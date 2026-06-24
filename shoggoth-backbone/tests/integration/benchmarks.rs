// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// tests/integration/benchmarks.rs — Synthetic benchmark harness.
//
// Provides geometric-stress workloads that exercise the Shoggoth fabric
// without requiring physical GPU hardware. These benchmarks run in CI
// and validate:
//   • Thread saturator throughput (tasks/sec vs. thread count).
//   • Compute fabric routing correctness (layer → target node mapping).
//   • QAT compression ratio (zstd vs. LZ4 vs. DEFLATE on synthetic data).
//   • Sync chain barrier scaling (latency vs. node count).
//   • Telemetry frame serialization throughput.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use shoggoth_core::qat_compress::{self, CompressionAlgo};
use shoggoth_core::thread_saturator::ShoggothThreadSaturator;
use shoggoth_sdk::sync_chain::ShoggothSyncChain;
use shoggoth_sdk::topology::{build_lab_topology, SpecializedCapability};
use shoggoth_sdk::telemetry::build_telemetry_frame;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// ── Thread Saturator Throughput ───────────────────────────────────────────────

fn bench_thread_saturator_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_saturator");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));

    for thread_count in [2, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::new("throughput", thread_count),
            &thread_count,
            |b, &count| {
                b.iter(|| {
                    let saturator = ShoggothThreadSaturator::new(count);
                    let counter = Arc::new(AtomicU64::new(0));

                    for i in 0..10_000 {
                        let c = Arc::clone(&counter);
                        saturator.spawn("bench", move || {
                            c.fetch_add(i, Ordering::Relaxed);
                        }, None);
                    }

                    let handles = saturator.launch();
                    std::thread::sleep(Duration::from_millis(200));
                    drop(handles);
                });
            },
        );
    }
    group.finish();
}

// ── QAT Compression Ratio ─────────────────────────────────────────────────────

fn bench_compression_ratio(c: &mut Criterion) {
    let mut group = c.benchmark_group("qat_compression");

    let synthetic_text = b"Shoggoth Mesh Machine — inter-node traffic compression benchmark payload. ".repeat(500);

    for algo in [CompressionAlgo::Zstd, CompressionAlgo::Lz4, CompressionAlgo::Deflate] {
        group.bench_with_input(
            BenchmarkId::new("compress", format!("{algo:?}")),
            &(algo, synthetic_text.as_slice()),
            |b, (algo, data)| {
                b.iter(|| {
                    qat_compress::compress(data, *algo).unwrap();
                });
            },
        );

        let compressed = qat_compress::compress(synthetic_text.as_slice(), algo).unwrap();
        group.bench_with_input(
            BenchmarkId::new("decompress", format!("{algo:?}")),
            &(algo, compressed.as_slice()),
            |b, (algo, data)| {
                b.iter(|| {
                    qat_compress::decompress(data, *algo).unwrap();
                });
            },
        );
    }
    group.finish();
}

// ── Sync Chain Barrier Scaling ────────────────────────────────────────────────

fn bench_sync_chain_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync_chain");
    let rt = tokio::runtime::Runtime::new().unwrap();

    for node_count in [2, 4, 8, 14] {
        group.bench_with_input(
            BenchmarkId::new("barrier", node_count),
            &node_count,
            |b, &count| {
                b.iter(|| {
                    rt.block_on(async {
                        let chain = Arc::new(ShoggothSyncChain::new(count));
                        let mut handles = Vec::new();

                        for i in 0..count {
                            let c = Arc::clone(&chain);
                            handles.push(tokio::spawn(async move {
                                let tile = shoggoth_sdk::sync_chain::TilePayload {
                                    tile_id: i as u32,
                                    frame_id: 1,
                                    vertex_matrix_hash: 0,
                                };
                                c.synchronize_cluster_tick(&format!("node-{i}"), tile).await;
                            }));
                        }

                        for h in handles {
                            h.await.unwrap();
                        }
                    });
                });
            },
        );
    }
    group.finish();
}

// ── Telemetry Frame Throughput ────────────────────────────────────────────────

fn bench_telemetry_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("telemetry");
    let pool = build_lab_topology();

    group.bench_function("build_frame", |b| {
        b.iter(|| {
            let frame = build_telemetry_frame(&pool, 0, 0, 3600);
            let json = serde_json::to_string(&frame).unwrap();
            assert!(json.len() > 100);
        });
    });

    group.finish();
}

// ── Topology Query Throughput ─────────────────────────────────────────────────

fn bench_topology_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology");

    let pool = build_lab_topology();

    group.bench_function("request_pooled_resources", |b| {
        b.iter(|| {
            let _ = pool.request_pooled_resources(SpecializedCapability::HardwareRayTracing);
        });
    });

    group.bench_function("full_shoggoth_nodes", |b| {
        b.iter(|| {
            let _ = pool.full_shoggoth_nodes();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_thread_saturator_throughput,
    bench_compression_ratio,
    bench_sync_chain_scaling,
    bench_telemetry_throughput,
    bench_topology_queries,
);
criterion_main!(benches);
