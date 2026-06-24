/**
 * @file shoggoth_sdk.cpp
 * @brief C++ wrapper around the Shoggoth C SDK.
 *
 * Provides RAII wrappers and std::string ergonomics on top of the C ABI.
 * Include `shoggoth_sdk.hpp` instead of `shoggoth_sdk.h` for C++ projects.
 */

#ifndef SHOGGOTH_SDK_HPP
#define SHOGGOTH_SDK_HPP

#include "shoggoth_sdk.h"

#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace shoggoth {

// ── Exception ─────────────────────────────────────────────────────────────────

class Error : public std::runtime_error {
public:
    explicit Error(const std::string& msg) : std::runtime_error(msg) {}
};

// ── Client ────────────────────────────────────────────────────────────────────

class Client {
public:
    explicit Client(const std::string& base_url = "http://localhost:9100") {
        shoggoth_error_t* err = shoggoth_client_connect(base_url.c_str(), &client_);
        if (err) {
            std::string msg = shoggoth_error_message(err);
            shoggoth_error_free(err);
            throw Error(msg);
        }
    }

    ~Client() { shoggoth_client_free(client_); }
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;
    Client(Client&& other) noexcept : client_(other.client_) { other.client_ = nullptr; }
    Client& operator=(Client&& other) noexcept {
        if (this != &other) { shoggoth_client_free(client_); client_ = other.client_; other.client_ = nullptr; }
        return *this;
    }

    shoggoth_client_t* raw() { return client_; }

private:
    shoggoth_client_t* client_ = nullptr;
};

// ── Node Info ─────────────────────────────────────────────────────────────────

struct NodeInfo {
    std::string node_id;
    std::string tier;
    uint32_t vram_gb = 0;
    float ping_ms = 0.0f;
    bool accepting_work = false;
    float temperature_c = 0.0f;
};

// ── Topology ──────────────────────────────────────────────────────────────────

class Topology {
public:
    explicit Topology(Client& client) {
        shoggoth_error_t* err = shoggoth_get_topology(client.raw(), &topo_);
        if (err) {
            std::string msg = shoggoth_error_message(err);
            shoggoth_error_free(err);
            throw Error(msg);
        }
    }

    ~Topology() { shoggoth_topology_free(topo_); }

    size_t node_count() const { return shoggoth_topology_node_count(topo_); }
    double total_vram_gb() const { return shoggoth_topology_total_vram_gb(topo_); }
    size_t full_shoggoths() const { return shoggoth_topology_full_shoggoths(topo_); }
    uint64_t uptime_seconds() const { return shoggoth_topology_uptime_seconds(topo_); }

    std::vector<NodeInfo> nodes() const {
        std::vector<NodeInfo> result;
        size_t n = node_count();
        result.reserve(n);
        for (size_t i = 0; i < n; i++) {
            auto* node = shoggoth_topology_node_at(topo_, i);
            result.push_back({
                shoggoth_node_id(node),
                shoggoth_node_tier(node),
                shoggoth_node_vram_gb(node),
                shoggoth_node_ping_ms(node),
                shoggoth_node_accepting_work(node),
                shoggoth_node_temperature_c(node),
            });
        }
        return result;
    }

private:
    shoggoth_topology_t* topo_ = nullptr;
};

// ── Analysis ──────────────────────────────────────────────────────────────────

struct AnalysisResult {
    std::string workload;
    std::string target_node;
    std::string reason;
    std::string template_name;
    std::string manifest;
    float confidence = 0.0f;
};

class Analysis {
public:
    Analysis(Client& client, const std::string& source_code) {
        shoggoth_error_t* err = shoggoth_analyze(client.raw(), source_code.c_str(), &analysis_);
        if (err) {
            std::string msg = shoggoth_error_message(err);
            shoggoth_error_free(err);
            throw Error(msg);
        }
    }

    ~Analysis() { shoggoth_analysis_free(analysis_); }

    AnalysisResult result() const {
        return {
            shoggoth_analysis_workload(analysis_),
            shoggoth_analysis_target_node(analysis_),
            shoggoth_analysis_reason(analysis_),
            shoggoth_analysis_template(analysis_),
            shoggoth_analysis_manifest(analysis_),
            shoggoth_analysis_confidence(analysis_),
        };
    }

private:
    shoggoth_analysis_t* analysis_ = nullptr;
};

// ── Dispatch ──────────────────────────────────────────────────────────────────

struct DispatchResult {
    bool success = false;
    uint64_t elapsed_us = 0;
    std::vector<uint8_t> output_data;
    std::string error_message;
};

class ComputeDispatch {
public:
    ComputeDispatch(
        Client& client,
        const std::vector<uint8_t>& spirv,
        const std::vector<uint8_t>& push_constants,
        const std::vector<uint8_t>& input_a,
        const std::vector<uint8_t>& input_b,
        uint32_t grid_x, uint32_t grid_y, uint32_t grid_z)
    {
        shoggoth_error_t* err = shoggoth_dispatch_compute(
            client.raw(),
            spirv.data(), spirv.size(),
            push_constants.data(), push_constants.size(),
            input_a.data(), input_a.size(),
            input_b.empty() ? nullptr : input_b.data(), input_b.size(),
            grid_x, grid_y, grid_z,
            &result_);
        if (err) {
            std::string msg = shoggoth_error_message(err);
            shoggoth_error_free(err);
            throw Error(msg);
        }
    }

    ~ComputeDispatch() { shoggoth_dispatch_result_free(result_); }

    DispatchResult result() const {
        DispatchResult r;
        r.success = shoggoth_dispatch_success(result_);
        r.elapsed_us = shoggoth_dispatch_elapsed_us(result_);
        size_t out_size = shoggoth_dispatch_output_size(result_);
        r.output_data.resize(out_size);
        shoggoth_dispatch_copy_output(result_, r.output_data.data(), out_size);
        r.error_message = shoggoth_dispatch_error_message(result_);
        return r;
    }

private:
    shoggoth_dispatch_result_t* result_ = nullptr;
};

} // namespace shoggoth

#endif // SHOGGOTH_SDK_HPP
