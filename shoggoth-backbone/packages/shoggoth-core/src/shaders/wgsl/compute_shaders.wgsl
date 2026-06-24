// ============================================================================
// Shoggoth WGSL Compute Shaders
// ============================================================================
//
// WebGPU Shading Language (WGSL) variants of the Shoggoth compute kernels.
// These compile natively on WebGPU implementations (Dawn, wgpu, browser).
// Unlike GLSL→SPIR-V, WGSL compiles directly without shaderc.
//
// Files:
//   gemm_benchmark.wgsl — Matrix multiply benchmark (WGSL equivalent of gemm_benchmark.comp.glsl)
//   frame_blend.wgsl    — Multi-source frame compositing compute shader
//   spatial_hash.wgsl   — xxHash64 spatial hashing for delta compression

// ── GEMM Benchmark ────────────────────────────────────────────────────────────
//
// Tiled matrix multiply: C = alpha * A * B + beta * C
// M×K times K×N = M×N
// Tile size: 16×16 (matches NVIDIA warp + AMD wavefront)

// === gemm_benchmark.wgsl ===

struct PushConstants {
    M: u32,
    N: u32,
    K: u32,
    alpha: f32,
    beta: f32,
    iterations: u32,
};

@group(0) @binding(0) var<storage, read>  A: array<f32>;
@group(0) @binding(1) var<storage, read>  B: array<f32>;
@group(0) @binding(2) var<storage, read_write> C: array<f32>;

var<push_constant> pc: PushConstants;

// Threadgroup-shared tile memory.
var<workgroup> As: array<f32, 256>;  // 16×16
var<workgroup> Bs: array<f32, 256>;  // 16×16

const TILE_M: u32 = 16u;
const TILE_N: u32 = 16u;
const TILE_K: u32 = 16u;

@compute @workgroup_size(TILE_M, TILE_N, 1)
fn main(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let wg_row = wg_id.x;
    let wg_col = wg_id.y;
    let local_row = local_id.x;
    let local_col = local_id.y;

    let global_row = wg_row * TILE_M + local_row;
    let global_col = wg_col * TILE_N + local_col;

    let num_tiles_k = (pc.K + TILE_K - 1u) / TILE_K;

    var accum = 0.0f;

    // ── Tiled matrix multiply loop ──
    for (var iter: u32 = 0u; iter < pc.iterations; iter = iter + 1u) {
        accum = 0.0f;

        for (var tile_k: u32 = 0u; tile_k < num_tiles_k; tile_k = tile_k + 1u) {
            // Cooperative load A tile.
            let a_k = tile_k * TILE_K + local_col;
            let a_idx = local_row * TILE_K + local_col;
            if (global_row < pc.M && a_k < pc.K) {
                As[a_idx] = A[global_row * pc.K + a_k];
            } else {
                As[a_idx] = 0.0f;
            }

            // Cooperative load B tile.
            let b_k = tile_k * TILE_K + local_row;
            let b_idx = local_row * TILE_K + local_col;
            if (b_k < pc.K && global_col < pc.N) {
                Bs[b_idx] = B[b_k * pc.N + global_col];
            } else {
                Bs[b_idx] = 0.0f;
            }

            workgroupBarrier();

            // Compute partial dot product.
            for (var k: u32 = 0u; k < TILE_K; k = k + 1u) {
                let a_idx2 = local_row * TILE_K + k;
                let b_idx2 = k * TILE_N + local_col;
                accum = accum + As[a_idx2] * Bs[b_idx2];
            }

            workgroupBarrier();
        }
    }

    // ── Write result ──
    if (global_row < pc.M && global_col < pc.N) {
        let idx = global_row * pc.N + global_col;
        C[idx] = pc.alpha * accum + pc.beta * C[idx];
    }
}

// ── Frame Blend Compositor ────────────────────────────────────────────────────
//
// === frame_blend.wgsl ===
//
// Blends multiple source frame fragments into a single composited back-buffer.
// Each fragment has: src_x, src_y, dst_x, dst_y, width, height, rgba data.
// Performs alpha compositing: dst = src * src_alpha + dst * (1 - src_alpha)

struct FrameBlendPush {
    backbuffer_width: u32,
    backbuffer_height: u32,
    fragment_count: u32,
};

struct FragmentDescriptor {
    src_offset: u32,    // Byte offset into the source data buffer.
    dst_x: u32,
    dst_y: u32,
    width: u32,
    height: u32,
};

@group(0) @binding(0) var<storage, read> src_data: array<u32>;      // All fragments packed.
@group(0) @binding(1) var<storage, read_write> backbuffer: array<u32>; // RGBA8 packed.
@group(0) @binding(2) var<storage, read> fragments: array<FragmentDescriptor>;

// === spatial_hash.wgsl ===
//
// Computes xxHash64 of a tile for delta compression.
// If the hash matches the previous frame's hash, the tile is skipped (static region).

@group(0) @binding(0) var<storage, read> pixel_data: array<u32>;
@group(0) @binding(1) var<storage, read_write> hash_output: array<u32>;  // [hash_lo, hash_hi]

const XXH_PRIME64_1: u64 = 11400714785074694791u;
const XXH_PRIME64_2: u64 = 14029467366897019727u;
const XXH_PRIME64_3: u64 = 1609587929392839161u;
const XXH_PRIME64_4: u64 = 9650029242287828579u;
const XXH_PRIME64_5: u64 = 2870177450012600261u;

fn xxh64_rotl(x: u64, r: u32) -> u64 {
    return (x << r) | (x >> (64u - r));
}

fn xxh64_round(acc: u64, input: u64) -> u64 {
    var a = acc + input * XXH_PRIME64_2;
    a = xxh64_rotl(a, 31u);
    return a * XXH_PRIME64_1;
}

@compute @workgroup_size(256, 1, 1)
fn hash_tile(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixel_count = arrayLength(&pixel_data);
    var hash: u64 = XXH_PRIME64_5 + (pixel_count as u64) * 4u;

    // Simplified xxHash64 — each thread hashes its stripe.
    let stripe = gid.x;
    let stripe_size = (pixel_count + 255u) / 256u;
    let start = stripe * stripe_size;
    let end = min(start + stripe_size, pixel_count);

    var lane_hash = XXH_PRIME64_5;
    for (var i = start; i < end; i = i + 4u) {
        var k1: u64 = 0u;
        if (i < pixel_count) { k1 = k1 | (u64(pixel_data[i]) << 0u); }
        if (i + 1u < pixel_count) { k1 = k1 | (u64(pixel_data[i + 1u]) << 32u); }
        lane_hash = xxh64_round(lane_hash, k1);
    }

    // Store lane hash (combined by CPU side in production).
    if (stripe < 256u) {
        hash_output[stripe * 2u] = u32(lane_hash & 0xFFFFFFFFu);
        hash_output[stripe * 2u + 1u] = u32(lane_hash >> 32u);
    }
}
