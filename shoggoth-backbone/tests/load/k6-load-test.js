import http from "k6/http";
import { check, sleep, group } from "k6";
import { Trend, Rate, Counter, Gauge } from "k6/metrics";

// ── Shoggoth Orchestrator Load Test ───────────────────────────────────────────
//
// Tests the orchestrator REST API under concurrent load to validate:
//   • Health endpoint latency under stress.
//   • Topology endpoint throughput.
//   • Analyze endpoint concurrency.
//   • Memory/CPU stability over extended runs.
//
// Usage:
//   k6 run --vus 50 --duration 60s load_test.js
//
// Stages:
//   k6 run --vus 10 --duration 30s load_test.js   # Warm-up
//   k6 run --vus 100 --duration 5m load_test.js   # Stress test
//   k6 run --vus 500 --duration 1m load_test.js   # Spike test

const BASE_URL = __ENV.SHOGGOTH_URL || "http://localhost:9100";

// ── Custom Metrics ────────────────────────────────────────────────────────────

const healthLatency = new Trend("shoggoth_health_latency_ms");
const topologyLatency = new Trend("shoggoth_topology_latency_ms");
const analyzeLatency = new Trend("shoggoth_analyze_latency_ms");
const launchLatency = new Trend("shoggoth_launch_latency_ms");
const errorRate = new Rate("shoggoth_error_rate");
const requestsTotal = new Counter("shoggoth_requests_total");

// ── Test Configuration ────────────────────────────────────────────────────────

export const options = {
    stages: [
        { duration: "30s", target: 10 },   // Ramp up to 10 VUs.
        { duration: "1m", target: 50 },    // Hold at 50 VUs.
        { duration: "30s", target: 0 },    // Ramp down.
    ],
    thresholds: {
        "shoggoth_health_latency_ms": ["p(95)<100"],
        "shoggoth_topology_latency_ms": ["p(95)<200"],
        "shoggoth_error_rate": ["rate<0.01"],
    },
};

// ── Setup ─────────────────────────────────────────────────────────────────────

export function setup() {
    // Warm up: ensure the orchestrator is reachable.
    const res = http.get(`${BASE_URL}/health`);
    check(res, { "orchestrator reachable": (r) => r.status === 200 });
    return { startTime: Date.now() };
}

// ── Main Test ─────────────────────────────────────────────────────────────────

export default function () {
    group("Health Check", () => {
        const start = Date.now();
        const res = http.get(`${BASE_URL}/health`);
        healthLatency.add(Date.now() - start);
        requestsTotal.add(1);

        const ok = check(res, {
            "health status 200": (r) => r.status === 200,
            "health response ok": (r) => r.json("status") === "ok",
        });
        if (!ok) errorRate.add(1);
    });

    group("Topology Fetch", () => {
        const start = Date.now();
        const res = http.get(`${BASE_URL}/topology`);
        topologyLatency.add(Date.now() - start);
        requestsTotal.add(1);

        const ok = check(res, {
            "topology status 200": (r) => r.status === 200,
            "topology has nodes": (r) => r.json("total_nodes") >= 0,
        });
        if (!ok) errorRate.add(1);
    });

    group("Workload Analysis", () => {
        const payload = JSON.stringify({
            source_code: "import torch.nn as nn; model = nn.Linear(20, 20).cuda()",
            project_name: "k6-load-test",
        });

        const start = Date.now();
        const res = http.post(`${BASE_URL}/analyze`, payload, {
            headers: { "Content-Type": "application/json" },
        });
        analyzeLatency.add(Date.now() - start);
        requestsTotal.add(1);

        const ok = check(res, {
            "analyze status 200": (r) => r.status === 200,
            "analyze has workload": (r) => r.json("workload") !== "",
        });
        if (!ok) errorRate.add(1);
    });

    // Only some VUs hit the launch endpoint (expensive operation).
    if (Math.random() < 0.1) {
        group("Template Launch", () => {
            const payload = JSON.stringify({
                template_name: "render-farm",
                project_name: "k6-load-test",
            });

            const start = Date.now();
            const res = http.post(`${BASE_URL}/launch`, payload, {
                headers: { "Content-Type": "application/json" },
            });
            launchLatency.add(Date.now() - start);
            requestsTotal.add(1);

            check(res, {
                "launch status 200": (r) => r.status === 200,
            });
        });
    }

    // Simulate realistic think time between requests.
    sleep(0.1 + Math.random() * 0.4);
}

// ── Teardown ──────────────────────────────────────────────────────────────────

export function teardown(data) {
    const elapsed = (Date.now() - data.startTime) / 1000;
    console.log(`Load test completed in ${elapsed.toFixed(0)}s.`);
    console.log(`Total requests: ${requestsTotal}`);
    console.log(`Error rate: ${errorRate.rate}`);
}

// ── Smoke Test (Quick Validation) ─────────────────────────────────────────────
//
// Run: k6 run --vus 1 --iterations 10 load_test.js
//
// Validates all endpoints respond correctly before a full load test.

export function smokeTest() {
    const endpoints = [
        { name: "health", method: "GET", path: "/health", expectedStatus: 200 },
        { name: "topology", method: "GET", path: "/topology", expectedStatus: 200 },
    ];

    for (const ep of endpoints) {
        const res = http.request(ep.method, `${BASE_URL}${ep.path}`);
        check(res, {
            [`${ep.name} status`]: (r) => r.status === ep.expectedStatus,
        });
    }
}
