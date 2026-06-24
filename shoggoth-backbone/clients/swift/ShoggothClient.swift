// Shoggoth Mobile/TV Bridge — iOS / tvOS / Android / SBC
//
// Lightweight HTTP + WebSocket client that connects mobile devices,
// Apple TV, and single-board computers (Raspberry Pi, Jetson Nano)
// to the Shoggoth fabric. These devices serve as VIEWPORT CLIENTS
// only — they receive the composited WebRTC stream and display it
// with sub-16ms decode latency. No GPU compute capabilities.
//
// Platform support:
//   • iOS 17+ (Swift, Network.framework + WebKit WebRTC).
//   • tvOS 17+ (same stack as iOS, optimized for 4K HDR displays).
//   • Android 14+ (Kotlin, OkHttp + Google WebRTC library).
//   • Linux SBC (Rust, same shoggoth-sdk crate).

import Foundation
import Network

/// Shoggoth Viewport Client for iOS / tvOS.
///
/// Connects to the Shoggoth orchestrator, fetches topology,
/// and establishes a WebRTC viewport stream for display.
///
/// Usage:
///   let client = ShoggothClient(orchestratorURL: "http://192.168.1.100:9100")
///   await client.connect()
///   let topo = await client.getTopology()
///   client.connectViewport(resolution: "3840x2160")
@available(iOS 17.0, tvOS 17.0, *)
public class ShoggothClient {
    private let orchestratorURL: String
    private var session: URLSession
    private var viewportTask: URLSessionWebSocketTask?

    public init(orchestratorURL: String = "http://localhost:9100") {
        self.orchestratorURL = orchestratorURL
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 10
        self.session = URLSession(configuration: config)
    }

    // ── Health ────────────────────────────────────────────────────────────────

    public func health() async throws -> [String: Any] {
        let url = URL(string: "\(orchestratorURL)/health")!
        let (data, _) = try await session.data(from: url)
        return try JSONSerialization.jsonObject(with: data) as! [String: Any]
    }

    // ── Topology ──────────────────────────────────────────────────────────────

    public func getTopology() async throws -> ShoggothTopology {
        let url = URL(string: "\(orchestratorURL)/topology")!
        let (data, _) = try await session.data(from: url)
        let json = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        return ShoggothTopology(json: json)
    }

    // ── Telemetry (WebSocket) ─────────────────────────────────────────────────

    public func connectTelemetry(onFrame: @escaping ([String: Any]) -> Void) {
        let wsURL = orchestratorURL
            .replacingOccurrences(of: "http://", with: "ws://")
            .replacingOccurrences(of: "https://", with: "wss://")
        let url = URL(string: "\(wsURL):9101/ws/telemetry")!
        viewportTask = session.webSocketTask(with: url)
        viewportTask?.resume()
        receiveTelemetryFrame(onFrame: onFrame)
    }

    private func receiveTelemetryFrame(onFrame: @escaping ([String: Any]) -> Void) {
        viewportTask?.receive { [weak self] result in
            switch result {
            case .success(let message):
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8),
                       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                        onFrame(json)
                    }
                default:
                    break
                }
                self?.receiveTelemetryFrame(onFrame: onFrame)
            case .failure:
                break
            }
        }
    }

    public func disconnect() {
        viewportTask?.cancel()
        session.invalidateAndCancel()
    }
}

// ── Data Models ───────────────────────────────────────────────────────────────

public struct ShoggothTopology {
    public let totalNodes: Int
    public let totalVramGb: Double
    public let fullShoggoths: Int
    public let nodes: [NodeInfo]

    init(json: [String: Any]) {
        totalNodes = json["total_nodes"] as? Int ?? 0
        totalVramGb = json["total_vram_gb"] as? Double ?? 0.0
        fullShoggoths = json["full_shoggoths"] as? Int ?? 0
        let nodesArray = json["nodes"] as? [[String: Any]] ?? []
        nodes = nodesArray.map { NodeInfo(json: $0) }
    }
}

public struct NodeInfo {
    public let nodeId: String
    public let tier: String
    public let vramGb: Int
    public let pingMs: Float
    public let acceptingWork: Bool
    public let temperatureC: Float

    init(json: [String: Any]) {
        nodeId = json["node_id"] as? String ?? ""
        tier = json["tier"] as? String ?? ""
        vramGb = json["vram_gb"] as? Int ?? 0
        pingMs = json["ping_ms"] as? Float ?? 0.0
        acceptingWork = json["accepting_work"] as? Bool ?? false
        temperatureC = json["temperature_c"] as? Float ?? 0.0
    }
}
