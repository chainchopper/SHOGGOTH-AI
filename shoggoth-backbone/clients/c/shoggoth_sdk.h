/**
 * @file shoggoth_sdk.h
 * @brief C/C++ SDK for the Shoggoth Mesh Machine.
 *
 * Provides a zero-dependency C ABI for integrating game engines, simulation
 * frameworks, and native applications with the Shoggoth fabric.
 *
 * All functions return shoggoth_error_t (0 = success, non-zero = error code).
 * Thread-safe: all functions may be called from any thread.
 *
 * ## Quick Start
 *
 * ```c
 * #include <shoggoth_sdk.h>
 *
 * int main() {
 *     shoggoth_client_t* client = NULL;
 *     shoggoth_error_t err = shoggoth_client_connect("http://localhost:9100", &client);
 *     if (err) { shoggoth_error_print(err); return 1; }
 *
 *     shoggoth_topology_t* topo = NULL;
 *     shoggoth_get_topology(client, &topo);
 *     printf("Nodes: %zu, VRAM: %.1f GB\n", topo->node_count, topo->total_vram_gb);
 *     shoggoth_topology_free(topo);
 *
 *     shoggoth_analysis_t* analysis = NULL;
 *     shoggoth_analyze(client, "import torch.nn as nn", &analysis);
 *     printf("Workload: %s -> %s\n", analysis->workload, analysis->target_node);
 *     shoggoth_analysis_free(analysis);
 *
 *     shoggoth_client_free(client);
 *     return 0;
 * }
 * ```
 *
 * ## Linking
 *
 * ```cmake
 * find_package(shoggoth REQUIRED)
 * target_link_libraries(my_app PRIVATE shoggoth::sdk)
 * ```
 *
 * Or directly:
 * ```bash
 * gcc -I/opt/shoggoth/include -L/opt/shoggoth/lib -lshoggoth_sdk -o myapp myapp.c
 * ```
 *
 * @license Apache-2.0
 * @copyright 2026 Shoggoth Mesh Machine Contributors
 */

#ifndef SHOGGOTH_SDK_H
#define SHOGGOTH_SDK_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ────────────────────────────────────────────────────────────────── */

#define SHOGGOTH_SDK_VERSION_MAJOR 0
#define SHOGGOTH_SDK_VERSION_MINOR 1
#define SHOGGOTH_SDK_VERSION_PATCH 0
#define SHOGGOTH_SDK_VERSION_STRING "0.1.0"

/* ── Error Handling ─────────────────────────────────────────────────────────── */

/** Opaque error type. */
typedef struct shoggoth_error_t shoggoth_error_t;

/** Returns the error message as a null-terminated string. Caller must free with
 *  shoggoth_error_free(). */
const char* shoggoth_error_message(const shoggoth_error_t* error);

/** Frees an error object. */
void shoggoth_error_free(shoggoth_error_t* error);

/** Prints the error message to stderr. */
void shoggoth_error_print(const shoggoth_error_t* error);

/* ── Client ─────────────────────────────────────────────────────────────────── */

/** Opaque client handle. */
typedef struct shoggoth_client_t shoggoth_client_t;

/** Creates a new client connected to the orchestrator at `base_url`.
 *  Returns NULL and sets `error_out` on failure. */
shoggoth_error_t shoggoth_client_connect(
    const char* base_url,
    shoggoth_client_t** client_out);

/** Disconnects and frees the client. Safe to call with NULL. */
void shoggoth_client_free(shoggoth_client_t* client);

/** Returns the base URL the client is connected to. */
const char* shoggoth_client_base_url(const shoggoth_client_t* client);

/* ── Health ─────────────────────────────────────────────────────────────────── */

/** Performs a health check. Returns 0 on success. */
shoggoth_error_t shoggoth_health_check(
    shoggoth_client_t* client,
    char** status_out,
    char** version_out);

/* ── Topology ───────────────────────────────────────────────────────────────── */

/** Opaque topology snapshot. */
typedef struct shoggoth_topology_t shoggoth_topology_t;

/** Opaque node info within a topology. */
typedef struct shoggoth_node_info_t shoggoth_node_info_t;

/** Fetches the current hardware fabric topology. */
shoggoth_error_t shoggoth_get_topology(
    shoggoth_client_t* client,
    shoggoth_topology_t** topo_out);

/** Frees a topology snapshot. */
void shoggoth_topology_free(shoggoth_topology_t* topo);

/** Returns the total node count. */
size_t shoggoth_topology_node_count(const shoggoth_topology_t* topo);

/** Returns total VRAM in gigabytes. */
double shoggoth_topology_total_vram_gb(const shoggoth_topology_t* topo);

/** Returns the count of Full Shoggoth certified nodes. */
size_t shoggoth_topology_full_shoggoths(const shoggoth_topology_t* topo);

/** Returns the orchestrator uptime in seconds. */
uint64_t shoggoth_topology_uptime_seconds(const shoggoth_topology_t* topo);

/** Returns the node at `index` (0-based). Returns NULL if out of bounds. */
const shoggoth_node_info_t* shoggoth_topology_node_at(
    const shoggoth_topology_t* topo,
    size_t index);

/* ── Node Info Accessors ────────────────────────────────────────────────────── */

/** Node unique identifier. */
const char* shoggoth_node_id(const shoggoth_node_info_t* node);

/** Infrastructure tier (e.g., "EdgeOnPrem", "CloudScale"). */
const char* shoggoth_node_tier(const shoggoth_node_info_t* node);

/** Available VRAM in gigabytes. */
uint32_t shoggoth_node_vram_gb(const shoggoth_node_info_t* node);

/** Network ping in milliseconds. */
float shoggoth_node_ping_ms(const shoggoth_node_info_t* node);

/** Whether the node is accepting work. */
bool shoggoth_node_accepting_work(const shoggoth_node_info_t* node);

/** GPU temperature in Celsius. */
float shoggoth_node_temperature_c(const shoggoth_node_info_t* node);

/* ── Workload Analysis ──────────────────────────────────────────────────────── */

/** Opaque analysis result. */
typedef struct shoggoth_analysis_t shoggoth_analysis_t;

/** Analyzes source code and returns hardware routing recommendation. */
shoggoth_error_t shoggoth_analyze(
    shoggoth_client_t* client,
    const char* source_code,
    shoggoth_analysis_t** analysis_out);

/** Frees an analysis result. */
void shoggoth_analysis_free(shoggoth_analysis_t* analysis);

/** Classified workload type. */
const char* shoggoth_analysis_workload(const shoggoth_analysis_t* analysis);

/** Recommended hardware target node. */
const char* shoggoth_analysis_target_node(const shoggoth_analysis_t* analysis);

/** Human-readable justification for the routing decision. */
const char* shoggoth_analysis_reason(const shoggoth_analysis_t* analysis);

/** Suggested SDK template name. */
const char* shoggoth_analysis_template(const shoggoth_analysis_t* analysis);

/** Generated shoggoth.toml manifest content. */
const char* shoggoth_analysis_manifest(const shoggoth_analysis_t* analysis);

/** Confidence score (0.0 to 1.0). */
float shoggoth_analysis_confidence(const shoggoth_analysis_t* analysis);

/* ── Workload Launch ────────────────────────────────────────────────────────── */

/** Launches a pre-configured workflow template.
 *  Valid templates: "render-farm", "heavy-compute", "async-game-runtime",
 *  "genomic-processing", "generic". */
shoggoth_error_t shoggoth_launch_template(
    shoggoth_client_t* client,
    const char* template_name,
    const char* project_name,
    char** status_out,
    char** manifest_out,
    char** message_out);

/* ── Direct Compute Dispatch (Low-Level) ────────────────────────────────────── */

/** Opaque dispatch result. */
typedef struct shoggoth_dispatch_result_t shoggoth_dispatch_result_t;

/** Dispatches a compute shader (SPIR-V binary) to the fabric.
 *
 *  @param client         Connected orchestrator client.
 *  @param spirv_binary   Raw SPIR-V bytecode.
 *  @param spirv_size     Size of spirv_binary in bytes.
 *  @param push_constants Raw push constant data (max 64 bytes).
 *  @param push_size      Size of push_constants (0 = none).
 *  @param input_a        First input buffer (matrix A, etc.).
 *  @param input_a_size   Size of input_a in bytes.
 *  @param input_b        Second input buffer (matrix B, etc.). May be NULL.
 *  @param input_b_size   Size of input_b in bytes (0 if NULL).
 *  @param grid_x         Workgroup count in X dimension.
 *  @param grid_y         Workgroup count in Y dimension.
 *  @param grid_z         Workgroup count in Z dimension.
 *  @param result_out     On success, filled with the dispatch result.
 *  @return               0 on success, error code on failure.
 */
shoggoth_error_t shoggoth_dispatch_compute(
    shoggoth_client_t* client,
    const uint8_t* spirv_binary,
    size_t spirv_size,
    const uint8_t* push_constants,
    size_t push_size,
    const uint8_t* input_a,
    size_t input_a_size,
    const uint8_t* input_b,
    size_t input_b_size,
    uint32_t grid_x,
    uint32_t grid_y,
    uint32_t grid_z,
    shoggoth_dispatch_result_t** result_out);

/** Frees a dispatch result. */
void shoggoth_dispatch_result_free(shoggoth_dispatch_result_t* result);

/** Whether the dispatch completed successfully. */
bool shoggoth_dispatch_success(const shoggoth_dispatch_result_t* result);

/** Execution wall-clock time in microseconds. */
uint64_t shoggoth_dispatch_elapsed_us(const shoggoth_dispatch_result_t* result);

/** Size of the output data in bytes. */
size_t shoggoth_dispatch_output_size(const shoggoth_dispatch_result_t* result);

/** Copies the output data into `buffer` (caller-allocated, at least
 *  output_size bytes). Returns the number of bytes copied. */
size_t shoggoth_dispatch_copy_output(
    const shoggoth_dispatch_result_t* result,
    uint8_t* buffer,
    size_t buffer_size);

/** Error message if dispatch failed, or empty string. */
const char* shoggoth_dispatch_error_message(const shoggoth_dispatch_result_t* result);

#ifdef __cplusplus
}
#endif

#endif /* SHOGGOTH_SDK_H */
