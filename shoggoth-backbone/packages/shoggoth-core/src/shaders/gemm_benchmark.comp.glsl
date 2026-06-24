// SPDX-License-Identifier: Apache-2.0
// shoggoth-core/src/shaders/gemm_benchmark.comp.glsl
//
// GLSL Compute Shader: GEMM (General Matrix Multiply) Benchmark
//
// This shader is compiled to SPIR-V at build time via shaderc-rs and used
// as a standardized workload to benchmark cross-vendor compute throughput.
//
// C = alpha * A * B + beta * C
// where A is M×K, B is K×N, C is M×N.
//
// Optimized for:
//   • NVIDIA: maps to Tensor Cores via cooperative matrix extensions.
//   • AMD (BC250 RDNA2, MI50 CDNA): maps to Matrix Core Engine.
//   • Intel: maps to XMX engines on Arc GPUs.
//
// Compile: glslangValidator -V -o gemm_benchmark.spv gemm_benchmark.comp.glsl

#version 460
#extension GL_GOOGLE_include_directive : enable
#extension GL_KHR_shader_subgroup_basic : enable
#extension GL_KHR_shader_subgroup_arithmetic : enable
#extension GL_KHR_shader_subgroup_shuffle : enable
#extension GL_KHR_cooperative_matrix : enable

// ── Workgroup Configuration ────────────────────────────────────────────────────
layout(local_size_x_id = 0, local_size_y_id = 1, local_size_z = 1) in;

// Tile dimensions (tunable via specialization constants).
// Default: 16×16 tile, good for NVIDIA Turing+ and AMD RDNA2+.
layout(constant_id = 2) const uint TILE_M = 16;
layout(constant_id = 3) const uint TILE_N = 16;
layout(constant_id = 4) const uint TILE_K = 16;

// ── Push Constants ─────────────────────────────────────────────────────────────
layout(push_constant) uniform PushConstants {
    uint M;          // Rows of A and C
    uint N;          // Columns of B and C
    uint K;          // Columns of A, rows of B
    float alpha;     // Scalar multiplier for A*B
    float beta;      // Scalar multiplier for C
    uint iterations; // Number of times to repeat the computation for timing
} pc;

// ── Buffer Bindings ────────────────────────────────────────────────────────────
layout(set = 0, binding = 0) readonly buffer MatrixA {
    float data_a[]; // M × K, row-major
} A;

layout(set = 0, binding = 1) readonly buffer MatrixB {
    float data_b[]; // K × N, row-major
} B;

layout(set = 0, binding = 2) buffer MatrixC {
    float data_c[]; // M × N, row-major
} C;

// ── Shared Memory (Threadgroup Tile) ───────────────────────────────────────────
shared float As[TILE_M][TILE_K];
shared float Bs[TILE_K][TILE_N];

// ── Main ───────────────────────────────────────────────────────────────────────

void main() {
    // Global workgroup ID.
    uint wg_row = gl_WorkGroupID.x; // Which row of tiles in the M dimension.
    uint wg_col = gl_WorkGroupID.y; // Which column of tiles in the N dimension.

    // Local thread ID within the workgroup.
    uint local_row = gl_LocalInvocationID.x; // 0..TILE_M-1
    uint local_col = gl_LocalInvocationID.y; // 0..TILE_N-1

    // Global position in the output matrix.
    uint global_row = wg_row * TILE_M + local_row;
    uint global_col = wg_col * TILE_N + local_col;

    // Accumulator for this thread's output element.
    float accum = 0.0;

    // Number of tiles in the K dimension.
    uint num_tiles_k = (pc.K + TILE_K - 1) / TILE_K;

    // ── Tiled Matrix Multiply ──
    for (uint iter = 0; iter < pc.iterations; iter++) {
        accum = 0.0;

        for (uint tile_k = 0; tile_k < num_tiles_k; tile_k++) {
            // Cooperative load of A tile into shared memory.
            uint a_k = tile_k * TILE_K + local_col;
            if (global_row < pc.M && a_k < pc.K) {
                As[local_row][local_col] = A.data_a[global_row * pc.K + a_k];
            } else {
                As[local_row][local_col] = 0.0;
            }

            // Cooperative load of B tile into shared memory.
            uint b_k = tile_k * TILE_K + local_row;
            if (b_k < pc.K && global_col < pc.N) {
                Bs[local_row][local_col] = B.data_b[b_k * pc.N + global_col];
            } else {
                Bs[local_row][local_col] = 0.0;
            }

            barrier(); // Synchronize: all threads must finish loading.
            memoryBarrierShared();

            // Compute partial dot product.
            for (uint k = 0; k < TILE_K; k++) {
                accum += As[local_row][k] * Bs[k][local_col];
            }

            barrier(); // Synchronize before loading next tile.
            memoryBarrierShared();
        }
    }

    // ── Write Result ──
    if (global_row < pc.M && global_col < pc.N) {
        uint idx = global_row * pc.N + global_col;
        C.data_c[idx] = pc.alpha * accum + pc.beta * C.data_c[idx];
    }
}
