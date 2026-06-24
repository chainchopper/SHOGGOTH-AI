using System;
using System.Collections.Generic;
using System.Net.Http;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Shoggoth
{
    /// <summary>
    /// Shoggoth Mesh Machine — C# client for Unity and .NET applications.
    ///
    /// Usage:
    ///   var client = new ShoggothClient("http://localhost:9100");
    ///   var topo = await client.GetTopologyAsync();
    ///   var result = await client.AnalyzeAsync("import torch.nn as nn");
    /// </summary>
    public class ShoggothClient : IDisposable
    {
        private readonly HttpClient _http;
        private readonly string _baseUrl;
        private ClientWebSocket? _ws;

        public ShoggothClient(string baseUrl = "http://localhost:9100")
        {
            _baseUrl = baseUrl.TrimEnd('/');
            _http = new HttpClient { BaseAddress = new Uri(_baseUrl), Timeout = TimeSpan.FromSeconds(30) };
        }

        // ── Health ────────────────────────────────────────────────────────────

        public async Task<HealthResponse> HealthAsync()
        {
            var json = await GetAsync("/health");
            return JsonSerializer.Deserialize<HealthResponse>(json)!;
        }

        // ── Topology ──────────────────────────────────────────────────────────

        public async Task<TopologyResponse> GetTopologyAsync()
        {
            var json = await GetAsync("/topology");
            return JsonSerializer.Deserialize<TopologyResponse>(json)!;
        }

        public async Task<List<NodeInfo>> ListNodesAsync(string? capability = null)
        {
            var path = "/fabric/nodes";
            if (!string.IsNullOrEmpty(capability)) path += $"?capability={capability}";
            var json = await GetAsync(path);
            var doc = JsonDocument.Parse(json);
            return JsonSerializer.Deserialize<List<NodeInfo>>(doc.RootElement.GetProperty("nodes").GetRawText())!;
        }

        // ── Analysis ─────────────────────────────────────────────────────────

        public async Task<AnalysisResult> AnalyzeAsync(string sourceCode, string projectName = "")
        {
            var body = new { source_code = sourceCode, project_name = projectName };
            var json = await PostAsync("/analyze", body);
            return JsonSerializer.Deserialize<AnalysisResult>(json)!;
        }

        // ── Launch ───────────────────────────────────────────────────────────

        public async Task<LaunchResult> LaunchTemplateAsync(string templateName, string projectName = "")
        {
            var body = new { template_name = templateName, project_name = projectName };
            var json = await PostAsync("/launch", body);
            return JsonSerializer.Deserialize<LaunchResult>(json)!;
        }

        // ── Telemetry (WebSocket) ────────────────────────────────────────────

        public async Task ConnectTelemetryAsync(Func<TelemetryFrame, Task> onFrame, CancellationToken ct = default)
        {
            _ws = new ClientWebSocket();
            var uri = new Uri(_baseUrl.Replace("http://", "ws://").Replace("https://", "wss://") + ":9101/ws/telemetry");
            await _ws.ConnectAsync(uri, ct);

            var buffer = new byte[16384];
            while (_ws.State == WebSocketState.Open && !ct.IsCancellationRequested)
            {
                var result = await _ws.ReceiveAsync(new ArraySegment<byte>(buffer), ct);
                if (result.MessageType == WebSocketMessageType.Close) break;

                var json = Encoding.UTF8.GetString(buffer, 0, result.Count);
                if (json.StartsWith("FRAME:"))
                {
                    var frameJson = json.Substring(6);
                    var frame = JsonSerializer.Deserialize<TelemetryFrame>(frameJson);
                    if (frame != null) await onFrame(frame);
                }
            }
        }

        // ── Helpers ──────────────────────────────────────────────────────────

        private async Task<string> GetAsync(string path)
        {
            var response = await _http.GetAsync(path);
            response.EnsureSuccessStatusCode();
            return await response.Content.ReadAsStringAsync();
        }

        private async Task<string> PostAsync(string path, object body)
        {
            var json = JsonSerializer.Serialize(body);
            var content = new StringContent(json, Encoding.UTF8, "application/json");
            var response = await _http.PostAsync(path, content);
            response.EnsureSuccessStatusCode();
            return await response.Content.ReadAsStringAsync();
        }

        public void Dispose()
        {
            _ws?.Dispose();
            _http.Dispose();
        }
    }

    // ── Data Models ──────────────────────────────────────────────────────────

    public class HealthResponse
    {
        public string Status { get; set; } = "";
        public string Service { get; set; } = "";
        public string Version { get; set; } = "";
        public int Protocol { get; set; }
    }

    public class TopologyResponse
    {
        public int TotalNodes { get; set; }
        public double TotalVramGb { get; set; }
        public int FullShoggoths { get; set; }
        public List<NodeInfo> Nodes { get; set; } = new();
        public long UptimeSeconds { get; set; }
    }

    public class NodeInfo
    {
        public string NodeId { get; set; } = "";
        public string Tier { get; set; } = "";
        public int VramGb { get; set; }
        public float PingMs { get; set; }
        public bool AcceptingWork { get; set; }
        public float TemperatureC { get; set; }
    }

    public class AnalysisResult
    {
        public string Workload { get; set; } = "";
        public string TargetNode { get; set; } = "";
        public string Reason { get; set; } = "";
        public string SuggestedTemplate { get; set; } = "";
        public string TemplateManifest { get; set; } = "";
        public float Confidence { get; set; }
    }

    public class LaunchResult
    {
        public string Status { get; set; } = "";
        public string Template { get; set; } = "";
        public string Manifest { get; set; } = "";
        public string Message { get; set; } = "";
    }

    public class TelemetryFrame
    {
        public long Seq { get; set; }
        public double TimestampSecs { get; set; }
        public List<TelemetryNode> Nodes { get; set; } = new();
        public AggregateMetrics Aggregate { get; set; } = new();
    }

    public class TelemetryNode
    {
        public string NodeId { get; set; } = "";
        public int VramGb { get; set; }
        public float TemperatureC { get; set; }
        public float UtilizationPct { get; set; }
        public int QueueDepth { get; set; }
        public float PingMs { get; set; }
        public bool AcceptingWork { get; set; }
        public string Vendor { get; set; } = "";
    }

    public class AggregateMetrics
    {
        public int TotalNodes { get; set; }
        public int OnlineNodes { get; set; }
        public double TotalVramGb { get; set; }
        public int FullShoggoths { get; set; }
        public long ActiveWorkUnits { get; set; }
        public long UptimeSeconds { get; set; }
    }
}
