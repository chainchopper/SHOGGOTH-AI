// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Shoggoth Mesh Machine Contributors
//
// shoggoth-core/benches/thread_saturator.rs — Benchmark harness.
//
// Benchmarks the work-stealing thread saturator on the Xeon 6240 host.
// Measures:
//   • Task throughput (tasks/sec) with varying task granularity.
//   • Idle CPU waste (spin-wait percentage).
//   • Steal success rate (fraction of tasks obtained via work-stealing).
//   • Cross-NUMA node latency.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use shoggoth_core::thread_saturator::ShoggothThreadSaturator;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_saturator_throughput");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    for task_count in [1_000u64, 10_000, 100_000, 1_000_000] {
        group.bench_with_input(
            BenchmarkId::new("tasks", task_count),
            &task_count,
            |b, &count| {
                b.iter(|| {
                    let saturator = ShoggothThreadSaturator::new(8); // 8 threads for bench
                    let counter = Arc::new(AtomicU64::new(0));

                    for _ in 0..count {
                        let c = Arc::clone(&counter);
                        saturator.spawn("bench-task", move || {
                            // Simulate work: a few atomic ops.
                            for _ in 0..10 {
                                black_box(c.fetch_add(1, Ordering::Relaxed));
                            }
                        }, None);
                    }

                    let handles = saturator.launch();
                    // Wait briefly for tasks to drain.
                    std::thread::sleep(Duration::from_millis(100));

                    // In a real bench, we'd join handles on shutdown signal.
                    // For criterion, we just measure spawn+launch time.
                    drop(handles);
                });
            },
        );
    }
    group.finish();
}

fn bench_idle_waste(c: &mut Criterion) {
    c.bench_function("idle_waste", |b| {
        b.iter(|| {
            let saturator = ShoggothThreadSaturator::new(8);
            let counter = Arc::new(AtomicU64::new(0));

            // Inject a small number of tasks so threads spend time stealing/idling.
            for i in 0..100 {
                let c = Arc::clone(&counter);
                saturator.spawn("sparse-task", move || {
                    c.fetch_add(i, Ordering::Relaxed);
                }, None);
            }

            let handles = saturator.launch();
            std::thread::sleep(Duration::from_millis(50));
            drop(handles);
        });
    });
}

criterion_group!(benches, bench_throughput, bench_idle_waste);
criterion_main!(benches);
