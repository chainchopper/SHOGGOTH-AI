// Shoggoth Dashboard — Login / Authentication Component
//
// Handles OIDC login (Google/GitHub/Microsoft) and API key auth.
// Stores session token in localStorage; validates on mount.
//
// Usage: wrap your App with <AuthProvider><App /></AuthProvider>

import React, { createContext, useContext, useState, useEffect, useCallback } from "react";

// ── Types ──────────────────────────────────────────────────────────────────────

interface UserSession {
  session_id: string;
  email: string;
  name: string;
  role: "Admin" | "Operator" | "ReadOnly";
  provider: string;
  expires_at: number;
}

interface AuthContextType {
  session: UserSession | null;
  login: (provider: string) => void;
  loginWithKey: (apiKey: string) => Promise<boolean>;
  logout: () => void;
  isAuthenticated: boolean;
  isAdmin: boolean;
}

const AuthContext = createContext<AuthContextType>({
  session: null,
  login: () => {},
  loginWithKey: async () => false,
  logout: () => {},
  isAuthenticated: false,
  isAdmin: false,
});

// ── Provider ───────────────────────────────────────────────────────────────────

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [session, setSession] = useState<UserSession | null>(null);

  // Restore session from localStorage on mount.
  useEffect(() => {
    const stored = localStorage.getItem("shoggoth_session");
    if (stored) {
      try {
        const sess: UserSession = JSON.parse(stored);
        if (sess.expires_at > Date.now() / 1000) {
          setSession(sess);
        } else {
          localStorage.removeItem("shoggoth_session");
        }
      } catch { /* corrupted */ }
    }
  }, []);

  // Persist session to localStorage.
  useEffect(() => {
    if (session) {
      localStorage.setItem("shoggoth_session", JSON.stringify(session));
    } else {
      localStorage.removeItem("shoggoth_session");
    }
  }, [session]);

  // OIDC login: redirect to provider.
  const login = useCallback((provider: string) => {
    const orchestratorUrl = "http://localhost:9100";
    const redirectUri = window.location.origin + "/auth/callback";
    window.location.href = `${orchestratorUrl}/auth/${provider}?redirect_uri=${encodeURIComponent(redirectUri)}`;
  }, []);

  // API key login.
  const loginWithKey = useCallback(async (apiKey: string): Promise<boolean> => {
    try {
      const res = await fetch("http://localhost:9100/auth/validate", {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-Shoggoth-Key": apiKey },
      });
      if (!res.ok) return false;
      const data = await res.json();
      setSession({
        session_id: data.session_id,
        email: data.email || "api-key",
        name: data.name || "API Key User",
        role: data.role || "Operator",
        provider: "api-key",
        expires_at: data.expires_at || Date.now() / 1000 + 86400,
      });
      return true;
    } catch {
      return false;
    }
  }, []);

  const logout = useCallback(() => {
    setSession(null);
  }, []);

  return (
    <AuthContext.Provider
      value={{
        session,
        login,
        loginWithKey,
        logout,
        isAuthenticated: session !== null,
        isAdmin: session?.role === "Admin",
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export const useAuth = () => useContext(AuthContext);

// ── Login Screen Component ─────────────────────────────────────────────────────

export function LoginScreen() {
  const { login, loginWithKey, isAuthenticated } = useAuth();
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  if (isAuthenticated) return null; // Already logged in.

  const handleKeySubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);
    const ok = await loginWithKey(apiKey);
    setLoading(false);
    if (!ok) setError("Invalid API key. Check your key and try again.");
  };

  return (
    <div style={styles.overlay}>
      <div style={styles.card}>
        <h1 style={styles.title}>
          <span style={{ color: "var(--emerald)" }}>⬡</span> SHOGGOTH
        </h1>
        <p style={styles.subtitle}>Mesh Machine — Launchpad Dashboard</p>

        {/* OIDC Buttons */}
        <div style={styles.oidcSection}>
          <button onClick={() => login("google")} style={styles.oidcBtn}>
            Sign in with Google
          </button>
          <button onClick={() => login("github")} style={styles.oidcBtn}>
            Sign in with GitHub
          </button>
          <button onClick={() => login("microsoft")} style={styles.oidcBtn}>
            Sign in with Microsoft
          </button>
        </div>

        <div style={styles.divider}>
          <span style={styles.dividerText}>or use an API key</span>
        </div>

        {/* API Key */}
        <form onSubmit={handleKeySubmit} style={styles.keyForm}>
          <input
            type="password"
            placeholder="shoggoth-xxxx-xxxx-xxxx"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            style={styles.input}
            autoFocus
          />
          <button type="submit" disabled={loading || !apiKey} style={styles.submitBtn}>
            {loading ? "Validating..." : "Sign In"}
          </button>
          {error && <p style={styles.error}>{error}</p>}
        </form>
      </div>
    </div>
  );
}

// ── Styles ─────────────────────────────────────────────────────────────────────

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    position: "fixed",
    inset: 0,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    background: "var(--steel-dark)",
    zIndex: 9999,
  },
  card: {
    background: "var(--steel)",
    border: "1px solid var(--border)",
    borderRadius: "1rem",
    padding: "2.5rem",
    width: "100%",
    maxWidth: "420px",
    textAlign: "center",
  },
  title: {
    fontSize: "1.5rem",
    fontWeight: 700,
    color: "var(--text-primary)",
    marginBottom: "0.25rem",
  },
  subtitle: {
    fontSize: "0.8rem",
    color: "var(--text-secondary)",
    marginBottom: "2rem",
  },
  oidcSection: {
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
    marginBottom: "1.5rem",
  },
  oidcBtn: {
    padding: "0.75rem",
    background: "var(--steel-light)",
    border: "1px solid var(--border)",
    borderRadius: "0.5rem",
    color: "var(--text-primary)",
    cursor: "pointer",
    fontSize: "0.85rem",
    fontFamily: "inherit",
  },
  divider: {
    display: "flex",
    alignItems: "center",
    margin: "1rem 0",
  },
  dividerText: {
    color: "var(--text-secondary)",
    fontSize: "0.7rem",
    margin: "0 auto",
  },
  keyForm: {
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
  },
  input: {
    padding: "0.75rem",
    background: "var(--steel-dark)",
    border: "1px solid var(--border)",
    borderRadius: "0.5rem",
    color: "var(--text-primary)",
    fontSize: "0.85rem",
    fontFamily: "monospace",
    outline: "none",
  },
  submitBtn: {
    padding: "0.75rem",
    background: "var(--emerald)",
    border: "none",
    borderRadius: "0.5rem",
    color: "var(--steel-dark)",
    fontWeight: 700,
    cursor: "pointer",
    fontSize: "0.85rem",
    fontFamily: "inherit",
  },
  error: {
    color: "var(--accent-red)",
    fontSize: "0.75rem",
    marginTop: "0.5rem",
  },
};
