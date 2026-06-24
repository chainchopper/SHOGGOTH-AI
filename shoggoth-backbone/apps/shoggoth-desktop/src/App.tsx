import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

// ── Types ──────────────────────────────────────────────────────────────────────

interface NodeInfo {
  node_id: string;
  tier: string;
  vram_gb: number;
  ping_ms: number;
  accepting_work: boolean;
  temperature_c: number;
}

interface ClusterMetrics {
  total_nodes: number;
  total_vram_gb: number;
  full_shoggoths: number;
}

interface TopologyResponse {
  total_nodes: number;
  total_vram_gb: number;
  full_shoggoths: number;
  nodes: NodeInfo[];
  uptime_seconds: number;
}

// ── App ────────────────────────────────────────────────────────────────────────

export default function App() {
  const [topology, setTopology] = useState<TopologyResponse | null>(null);
  const [metrics, setMetrics] = useState<ClusterMetrics | null>(null);
  const [launchMessage, setLaunchMessage] = useState("");
  const [activeTab, setActiveTab] = useState<"fabric" | "launchpad">("fabric");

  const refreshTopology = useCallback(async () => {
    try {
      const t = await invoke<string>("get_topology");
      const nodes: NodeInfo[] = JSON.parse(t);
      setTopology({
        total_nodes: nodes.length,
        total_vram_gb: nodes.reduce((s, n) => s + n.vram_gb, 0),
        full_shoggoths: nodes.filter(
          (n) => n.vram_gb >= 48 && n.ping_ms < 5 && n.accepting_work,
        ).length,
        nodes,
        uptime_seconds: 0,
      });

      const m = await invoke<string>("get_cluster_metrics");
      setMetrics(JSON.parse(m));
    } catch (err) {
      console.error("Failed to fetch topology:", err);
    }
  }, []);

  useEffect(() => {
    refreshTopology();
    const interval = setInterval(refreshTopology, 5000);
    return () => clearInterval(interval);
  }, [refreshTopology]);

  const launchTemplate = async (templateName: string) => {
    try {
      const msg = await invoke<string>("launch_workflow_template", {
        templateName,
      });
      setLaunchMessage(msg);
    } catch (err) {
      setLaunchMessage(`Error: ${err}`);
    }
  };

  return (
    <div style={styles.container}>
      {/* Header */}
      <header style={styles.header}>
        <h1 style={styles.title}>
          <span style={{ color: "var(--emerald)" }}>⬡</span> SHOGGOTH
          <span style={{ color: "var(--emerald)" }}> LAUNCHPAD</span>
        </h1>
        <div style={styles.headerMetrics}>
          {metrics && (
            <>
              <Metric label="NODES" value={metrics.total_nodes} />
              <Metric label="VRAM" value={`${metrics.total_vram_gb.toFixed(0)} GB`} />
              <Metric label="FULL" value={metrics.full_shoggoths} color="var(--emerald)" />
            </>
          )}
        </div>
      </header>

      {/* Tabs */}
      <nav style={styles.tabs}>
        {(["fabric", "launchpad"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              ...styles.tabButton,
              borderBottom:
                activeTab === tab ? "2px solid var(--emerald)" : "2px solid transparent",
              color: activeTab === tab ? "var(--emerald)" : "var(--text-secondary)",
            }}
          >
            {tab === "fabric" ? "🔬 HARDWARE FABRIC" : "🚀 LAUNCHPAD TEMPLATES"}
          </button>
        ))}
      </nav>

      {/* Content */}
      <main style={styles.content}>
        {activeTab === "fabric" && topology && (
          <FabricView nodes={topology.nodes} totalVram={topology.total_vram_gb} />
        )}
        {activeTab === "launchpad" && (
          <LaunchpadView onLaunch={launchTemplate} message={launchMessage} />
        )}
      </main>

      {/* Footer */}
      <footer style={styles.footer}>
        <span>Shoggoth Mesh Machine v0.1.0 — Emerald #00FF66</span>
        <span>
          {topology
            ? `${topology.total_nodes} nodes · ${topology.total_vram_gb.toFixed(0)} GB VRAM`
            : "Connecting..."}
        </span>
      </footer>
    </div>
  );
}

// ── Sub-Components ─────────────────────────────────────────────────────────────

function Metric({
  label,
  value,
  color,
}: {
  label: string;
  value: string | number;
  color?: string;
}) {
  return (
    <div style={styles.metric}>
      <div style={styles.metricLabel}>{label}</div>
      <div style={{ ...styles.metricValue, color: color || "var(--text-primary)" }}>
        {value}
      </div>
    </div>
  );
}

function FabricView({
  nodes,
  totalVram,
}: {
  nodes: NodeInfo[];
  totalVram: number;
}) {
  return (
    <div style={styles.fabricGrid}>
      {nodes.map((node) => (
        <div key={node.node_id} style={styles.nodeCard}>
          <div style={styles.nodeHeader}>
            <span
              style={{
                color: node.accepting_work ? "var(--emerald)" : "var(--accent-red)",
                fontSize: "0.75rem",
              }}
            >
              {node.accepting_work ? "● ONLINE" : "○ OFFLINE"}
            </span>
            <span style={{ color: "var(--text-secondary)", fontSize: "0.65rem" }}>
              {node.tier}
            </span>
          </div>
          <div style={styles.nodeName}>{node.node_id}</div>
          <div style={styles.nodeStats}>
            <span>{node.vram_gb} GB VRAM</span>
            <span>{node.ping_ms.toFixed(1)} ms</span>
            <span>{node.temperature_c.toFixed(0)}°C</span>
          </div>
        </div>
      ))}
    </div>
  );
}

function LaunchpadView({
  onLaunch,
  message,
}: {
  onLaunch: (template: string) => void;
  message: string;
}) {
  const templates = [
    {
      id: "async_game_runtime",
      name: "Async Game Runtime",
      description: "Split UI/Sim to Edge, Shadow/Ray Casting to Cloud. 16K ready.",
      icon: "🎮",
    },
    {
      id: "pytorch_hybrid_scale",
      name: "PyTorch Hybrid Compute",
      description: "Local weights sharding with remote epoch processing via BC250 grid.",
      icon: "🧠",
    },
    {
      id: "render_farm",
      name: "Render Farm",
      description: "BVH sharding to RT cores. BC250 grid as distributed rasterizers.",
      icon: "🎬",
    },
    {
      id: "genomic_pipeline",
      name: "Genomic Pipeline",
      description: "FASTA parsing + ScyllaDB shard-per-core + alignment vectorization.",
      icon: "🧬",
    },
  ];

  return (
    <div>
      <div style={styles.templateGrid}>
        {templates.map((t) => (
          <button
            key={t.id}
            onClick={() => onLaunch(t.id)}
            style={styles.templateCard}
          >
            <div style={{ fontSize: "2rem", marginBottom: "0.5rem" }}>{t.icon}</div>
            <div style={styles.templateName}>{t.name}</div>
            <div style={styles.templateDesc}>{t.description}</div>
          </button>
        ))}
      </div>
      {message && <div style={styles.launchMessage}>{">"} {message}</div>}
    </div>
  );
}

// ── Styles ─────────────────────────────────────────────────────────────────────

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    height: "100vh",
    background: "var(--steel-dark)",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.75rem 1.5rem",
    borderBottom: "1px solid var(--border)",
    background: "var(--steel)",
  },
  title: {
    fontSize: "1.25rem",
    fontWeight: 600,
    letterSpacing: "0.05em",
    color: "var(--text-primary)",
  },
  headerMetrics: {
    display: "flex",
    gap: "1.5rem",
  },
  metric: {
    textAlign: "center",
  },
  metricLabel: {
    fontSize: "0.55rem",
    color: "var(--text-secondary)",
    letterSpacing: "0.1em",
  },
  metricValue: {
    fontSize: "1rem",
    fontWeight: 700,
  },
  tabs: {
    display: "flex",
    background: "var(--steel)",
    padding: "0 1.5rem",
    borderBottom: "1px solid var(--border)",
  },
  tabButton: {
    background: "none",
    border: "none",
    color: "var(--text-secondary)",
    padding: "0.75rem 1.5rem",
    cursor: "pointer",
    fontSize: "0.8rem",
    fontFamily: "inherit",
    letterSpacing: "0.05em",
    fontWeight: 600,
  },
  content: {
    flex: 1,
    overflow: "auto",
    padding: "1rem 1.5rem",
  },
  fabricGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
    gap: "0.75rem",
  },
  nodeCard: {
    background: "var(--steel)",
    border: "1px solid var(--border)",
    borderRadius: "0.5rem",
    padding: "0.75rem",
  },
  nodeHeader: {
    display: "flex",
    justifyContent: "space-between",
    marginBottom: "0.5rem",
  },
  nodeName: {
    fontSize: "0.85rem",
    fontWeight: 600,
    color: "var(--text-primary)",
    marginBottom: "0.5rem",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  nodeStats: {
    display: "flex",
    gap: "0.75rem",
    fontSize: "0.7rem",
    color: "var(--text-secondary)",
  },
  templateGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
    gap: "0.75rem",
    marginBottom: "1rem",
  },
  templateCard: {
    background: "var(--steel)",
    border: "1px solid var(--border)",
    borderRadius: "0.5rem",
    padding: "1.5rem",
    cursor: "pointer",
    textAlign: "center",
    fontFamily: "inherit",
    color: "var(--text-primary)",
    transition: "border-color 0.2s",
  },
  templateName: {
    fontSize: "0.9rem",
    fontWeight: 600,
    color: "var(--emerald)",
  },
  templateDesc: {
    fontSize: "0.7rem",
    color: "var(--text-secondary)",
    marginTop: "0.25rem",
  },
  launchMessage: {
    background: "var(--steel)",
    border: "1px solid var(--emerald)",
    borderRadius: "0.5rem",
    padding: "1rem",
    fontFamily: "monospace",
    fontSize: "0.8rem",
    color: "var(--emerald)",
  },
  footer: {
    display: "flex",
    justifyContent: "space-between",
    padding: "0.5rem 1.5rem",
    borderTop: "1px solid var(--border)",
    background: "var(--steel)",
    fontSize: "0.65rem",
    color: "var(--text-secondary)",
  },
};
