// Shoggoth TypeScript API Client
//
// Type-safe HTTP + WebSocket client for the Shoggoth orchestrator.
// Designed for the Tauri Launchpad Dashboard and browser-based tools.
//
// Usage:
//   import { ShoggothClient } from "./shoggoth-client";
//
//   const client = new ShoggothClient("http://localhost:9100");
//   const topo = await client.getTopology();
//   const result = await client.analyze("import torch.nn as nn");

// ── Types ──────────────────────────────────────────────────────────────────────

export interface NodeInfo {
  node_id: string;
  tier: "EdgeOnPrem" | "CloudScale";
  vram_gb: number;
  ping_ms: number;
  accepting_work: boolean;
  temperature_c: number;
}

export interface TopologySnapshot {
  total_nodes: number;
  total_vram_gb: number;
  full_shoggoths: number;
  nodes: NodeInfo[];
  uptime_seconds: number;
}

export interface AnalysisResult {
  workload: string;
  target_node: string;
  reason: string;
  suggested_template: string;
  template_manifest: string;
  confidence: number;
}

export interface LaunchResult {
  status: string;
  template: string;
  manifest: string;
  message: string;
}

export interface TelemetryFrame {
  seq: number;
  timestamp_secs: number;
  nodes: {
    node_id: string;
    vram_gb: number;
    temperature_c: number;
    utilization_pct: number;
    queue_depth: number;
    ping_ms: number;
    accepting_work: boolean;
    vendor: string;
  }[];
  aggregate: {
    total_nodes: number;
    online_nodes: number;
    total_vram_gb: number;
    full_shoggoths: number;
    active_work_units: number;
    uptime_seconds: number;
  };
}

// ── HTTP Client ────────────────────────────────────────────────────────────────

export class ShoggothClient {
  private baseUrl: string;
  private telemetryWs: WebSocket | null = null;

  constructor(baseUrl = "http://localhost:9100") {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const res = await fetch(url, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    });
    if (!res.ok) {
      throw new Error(`Shoggoth API error ${res.status}: ${res.statusText}`);
    }
    return res.json();
  }

  // ── Endpoints ───────────────────────────────────────────────────────────────

  async health(): Promise<{ status: string; service: string; version: string }> {
    return this.request("/health");
  }

  async getTopology(): Promise<TopologySnapshot> {
    return this.request("/topology");
  }

  async listNodes(capability?: string): Promise<{ nodes: NodeInfo[]; count: number }> {
    const params = capability ? `?capability=${capability}` : "";
    return this.request(`/fabric/nodes${params}`);
  }

  async analyze(sourceCode: string, projectName = ""): Promise<AnalysisResult> {
    return this.request("/analyze", {
      method: "POST",
      body: JSON.stringify({ source_code: sourceCode, project_name: projectName }),
    });
  }

  async launch(templateName: string, projectName = ""): Promise<LaunchResult> {
    return this.request("/launch", {
      method: "POST",
      body: JSON.stringify({ template_name: templateName, project_name: projectName }),
    });
  }

  async registerNode(node: Partial<NodeInfo> & { node_id: string }): Promise<{ status: string }> {
    return this.request("/fabric/register", {
      method: "POST",
      body: JSON.stringify(node),
    });
  }

  // ── WebSocket Telemetry ─────────────────────────────────────────────────────

  connectTelemetry(
    wsBaseUrl = "ws://localhost:9101",
    onFrame: (frame: TelemetryFrame) => void,
    onError?: (err: Event) => void,
  ): () => void {
    const url = `${wsBaseUrl.replace(/\/$/, "")}/ws/telemetry`;
    const ws = new WebSocket(url);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.startsWith?.("FRAME:")) {
          const frame: TelemetryFrame = JSON.parse(data.slice(6));
          onFrame(frame);
        }
      } catch {
        // Skip malformed frames.
      }
    };

    ws.onerror = (err) => {
      if (onError) onError(err);
    };

    ws.onclose = () => {
      // Auto-reconnect after 2 seconds.
      setTimeout(() => {
        this.connectTelemetry(wsBaseUrl, onFrame, onError);
      }, 2000);
    };

    this.telemetryWs = ws;

    // Return a disconnect function.
    return () => {
      ws.close();
      this.telemetryWs = null;
    };
  }

  disconnectTelemetry(): void {
    if (this.telemetryWs) {
      this.telemetryWs.close();
      this.telemetryWs = null;
    }
  }
}
