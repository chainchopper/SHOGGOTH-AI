// Shoggoth Dashboard — Superadmin Panel
//
// Admin-only view: API key management, node control, cloud provisioning,
// system configuration, audit logs. Only rendered when role === "Admin".

import React, { useState, useEffect, useCallback } from "react";
import { useAuth } from "./LoginScreen";

// ── Types ──────────────────────────────────────────────────────────────────────

interface ApiKeyInfo {
  key_id: string;
  label: string;
  role: string;
  created_at: number;
}

interface SystemConfig {
  max_cloud_nodes: number;
  cloud_budget_hourly: number;
  scale_up_threshold: number;
  session_ttl_seconds: number;
  rate_limit_rps: number;
}

// ── Superadmin Panel ───────────────────────────────────────────────────────────

export function SuperadminPanel() {
  const { session, isAdmin } = useAuth();
  const [activeSection, setActiveSection] = useState<"keys" | "nodes" | "cloud" | "config" | "audit">("keys");

  if (!isAdmin) {
    return (
      <div style={styles.denied}>
        <h2>Access Denied</h2>
        <p>Superadmin panel requires Admin role. Your role: {session?.role || "none"}</p>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      <h2 style={styles.heading}>
        <span style={{ color: "var(--emerald)" }}>⬡</span> SUPERADMIN
      </h2>

      <nav style={styles.nav}>
        {(["keys", "nodes", "cloud", "config", "audit"] as const).map((s) => (
          <button
            key={s}
            onClick={() => setActiveSection(s)}
            style={{
              ...styles.navBtn,
              borderBottom: activeSection === s ? "2px solid var(--emerald)" : "2px solid transparent",
              color: activeSection === s ? "var(--emerald)" : "var(--text-secondary)",
            }}
          >
            {s === "keys" && "🔑 API Keys"}
            {s === "nodes" && "🖥 Nodes"}
            {s === "cloud" && "☁️ Cloud"}
            {s === "config" && "⚙️ Config"}
            {s === "audit" && "📋 Audit"}
          </button>
        ))}
      </nav>

      <main style={styles.content}>
        {activeSection === "keys" && <ApiKeyManager />}
        {activeSection === "nodes" && <NodeManager />}
        {activeSection === "cloud" && <CloudManager />}
        {activeSection === "config" && <ConfigManager />}
        {activeSection === "audit" && <AuditLog />}
      </main>
    </div>
  );
}

// ── API Key Manager ────────────────────────────────────────────────────────────

function ApiKeyManager() {
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [newLabel, setNewLabel] = useState("");
  const [newRole, setNewRole] = useState("Operator");
  const [newKey, setNewKey] = useState("");

  const generateKey = async () => {
    try {
      const res = await fetch("http://localhost:9100/admin/keys", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ label: newLabel || "untitled", role: newRole }),
      });
      const data = await res.json();
      setNewKey(data.raw_key);
      setKeys((prev) => [...prev, { key_id: data.key_id, label: newLabel, role: newRole, created_at: Date.now() }]);
    } catch (e) {
      console.error("Failed to generate key:", e);
    }
  };

  const revokeKey = async (keyId: string) => {
    await fetch(`http://localhost:9100/admin/keys/${keyId}`, { method: "DELETE" });
    setKeys((prev) => prev.filter((k) => k.key_id !== keyId));
  };

  return (
    <div>
      <h3>API Key Management</h3>
      <div style={styles.formRow}>
        <input
          placeholder="Key label (e.g., CI/CD pipeline)"
          value={newLabel}
          onChange={(e) => setNewLabel(e.target.value)}
          style={styles.input}
        />
        <select value={newRole} onChange={(e) => setNewRole(e.target.value)} style={styles.select}>
          <option value="Admin">Admin</option>
          <option value="Operator">Operator</option>
          <option value="ReadOnly">ReadOnly</option>
        </select>
        <button onClick={generateKey} style={styles.btn}>Generate</button>
      </div>

      {newKey && (
        <div style={styles.keyReveal}>
          <strong>New API Key (show once):</strong>
          <code style={styles.keyCode}>{newKey}</code>
        </div>
      )}

      <table style={styles.table}>
        <thead>
          <tr>
            <th>Key ID</th><th>Label</th><th>Role</th><th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((k) => (
            <tr key={k.key_id}>
              <td style={{ fontFamily: "monospace", fontSize: "0.75rem" }}>{k.key_id}</td>
              <td>{k.label}</td>
              <td style={{ color: k.role === "Admin" ? "var(--emerald)" : "var(--text-secondary)" }}>{k.role}</td>
              <td><button onClick={() => revokeKey(k.key_id)} style={styles.dangerBtn}>Revoke</button></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Node Manager ───────────────────────────────────────────────────────────────

function NodeManager() {
  const [nodes, setNodes] = useState<any[]>([]);

  const fetchNodes = async () => {
    const res = await fetch("http://localhost:9100/topology");
    const data = await res.json();
    setNodes(data.nodes || []);
  };

  useEffect(() => { fetchNodes(); }, []);

  const drainNode = async (nodeId: string) => {
    await fetch(`http://localhost:9100/admin/nodes/${nodeId}/drain`, { method: "POST" });
    fetchNodes();
  };

  return (
    <div>
      <h3>Node Control</h3>
      <table style={styles.table}>
        <thead>
          <tr><th>Node</th><th>Status</th><th>VRAM</th><th>Temp</th><th>Actions</th></tr>
        </thead>
        <tbody>
          {nodes.map((n: any) => (
            <tr key={n.node_id}>
              <td>{n.node_id}</td>
              <td style={{ color: n.accepting_work ? "var(--emerald)" : "var(--accent-red)" }}>
                {n.accepting_work ? "ONLINE" : "DRAINED"}
              </td>
              <td>{n.vram_gb} GB</td>
              <td>{n.temperature_c}°C</td>
              <td>
                <button onClick={() => drainNode(n.node_id)} style={styles.smallBtn}>Drain</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Cloud Manager ──────────────────────────────────────────────────────────────

function CloudManager() {
  const [count, setCount] = useState(0);
  const [budget, setBudget] = useState(50);

  const provisionCloud = async () => {
    await fetch("http://localhost:9100/admin/cloud/provision", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ capability: "MatrixTensorCore", count }),
    });
  };

  return (
    <div>
      <h3>Cloud Provisioning</h3>
      <div style={styles.formRow}>
        <label>Nodes: </label>
        <input type="number" min={1} max={16} value={count} onChange={(e) => setCount(+e.target.value)} style={{ ...styles.input, width: 80 }} />
        <button onClick={provisionCloud} style={styles.btn}>Provision</button>
      </div>
      <div style={styles.formRow}>
        <label>Budget: $</label>
        <input type="number" min={1} value={budget} onChange={(e) => setBudget(+e.target.value)} style={{ ...styles.input, width: 80 }} />
        <span style={{ color: "var(--text-secondary)", fontSize: "0.75rem" }}>/hr</span>
      </div>
    </div>
  );
}

// ── Config Manager ─────────────────────────────────────────────────────────────

function ConfigManager() {
  const [config, setConfig] = useState<SystemConfig>({
    max_cloud_nodes: 16,
    cloud_budget_hourly: 50,
    scale_up_threshold: 0.85,
    session_ttl_seconds: 3600,
    rate_limit_rps: 100,
  });

  const saveConfig = async () => {
    await fetch("http://localhost:9100/admin/config", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(config),
    });
  };

  return (
    <div>
      <h3>System Configuration</h3>
      {Object.entries(config).map(([key, value]) => (
        <div key={key} style={styles.formRow}>
          <label style={{ width: 200 }}>{key.replace(/_/g, " ")}:</label>
          <input
            type="number"
            value={value}
            onChange={(e) => setConfig((c) => ({ ...c, [key]: +e.target.value }))}
            style={{ ...styles.input, width: 120 }}
          />
        </div>
      ))}
      <button onClick={saveConfig} style={styles.btn}>Save Configuration</button>
    </div>
  );
}

// ── Audit Log ──────────────────────────────────────────────────────────────────

function AuditLog() {
  const [logs, setLogs] = useState<string[]>([]);

  useEffect(() => {
    fetch("http://localhost:9100/admin/audit")
      .then((r) => r.json())
      .then((d) => setLogs(d.entries || []))
      .catch(() => setLogs(["Failed to load audit log"]));
  }, []);

  return (
    <div>
      <h3>Audit Log</h3>
      <div style={styles.logContainer}>
        {logs.map((entry, i) => (
          <div key={i} style={styles.logEntry}>{entry}</div>
        ))}
      </div>
    </div>
  );
}

// ── Styles ─────────────────────────────────────────────────────────────────────

const styles: Record<string, React.CSSProperties> = {
  container: {},
  heading: { fontSize: "1.1rem", fontWeight: 600, marginBottom: "1rem", color: "var(--text-primary)" },
  denied: { padding: "2rem", textAlign: "center", color: "var(--accent-red)" },
  nav: { display: "flex", gap: "0.25rem", marginBottom: "1rem", borderBottom: "1px solid var(--border)" },
  navBtn: { background: "none", border: "none", padding: "0.5rem 1rem", cursor: "pointer", fontFamily: "inherit", fontSize: "0.8rem", fontWeight: 600 },
  content: {},
  formRow: { display: "flex", gap: "0.5rem", alignItems: "center", marginBottom: "0.5rem" },
  input: { padding: "0.5rem", background: "var(--steel-dark)", border: "1px solid var(--border)", borderRadius: "0.25rem", color: "var(--text-primary)", fontFamily: "monospace", fontSize: "0.8rem", outline: "none" },
  select: { padding: "0.5rem", background: "var(--steel-dark)", border: "1px solid var(--border)", borderRadius: "0.25rem", color: "var(--text-primary)", fontSize: "0.8rem" },
  btn: { padding: "0.5rem 1rem", background: "var(--emerald)", border: "none", borderRadius: "0.25rem", color: "var(--steel-dark)", fontWeight: 700, cursor: "pointer", fontFamily: "inherit" },
  smallBtn: { padding: "0.25rem 0.5rem", background: "var(--steel-light)", border: "1px solid var(--border)", borderRadius: "0.25rem", color: "var(--text-primary)", cursor: "pointer", fontSize: "0.7rem", fontFamily: "inherit" },
  dangerBtn: { padding: "0.25rem 0.5rem", background: "var(--accent-red)", border: "none", borderRadius: "0.25rem", color: "#fff", cursor: "pointer", fontSize: "0.7rem", fontFamily: "inherit" },
  table: { width: "100%", borderCollapse: "collapse", fontSize: "0.8rem" },
  keyReveal: { background: "var(--steel-dark)", border: "1px solid var(--emerald)", borderRadius: "0.5rem", padding: "0.75rem", marginBottom: "1rem" },
  keyCode: { display: "block", marginTop: "0.25rem", color: "var(--emerald)", wordBreak: "break-all" },
  logContainer: { background: "var(--steel-dark)", border: "1px solid var(--border)", borderRadius: "0.5rem", padding: "0.75rem", maxHeight: 400, overflow: "auto" },
  logEntry: { fontSize: "0.7rem", fontFamily: "monospace", color: "var(--text-secondary)", padding: "0.25rem 0", borderBottom: "1px solid var(--border)" },
};
