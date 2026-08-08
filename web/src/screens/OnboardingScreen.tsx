import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";

interface HandleInput {
  handle: string;
  service: string;
}

const SERVICES = ["phone", "email", "discord", "instagram", "telegram", "signal"];

export default function OnboardingScreen() {
  const { finishOnboarding, logout } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>([{ handle: "", service: "phone" }]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const addHandle = () => {
    setHandles([...handles, { handle: "", service: "phone" }]);
  };

  const updateHandle = (index: number, field: keyof HandleInput, value: string) => {
    const next = [...handles];
    next[index] = { ...next[index], [field]: value };
    setHandles(next);
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    setHandles(handles.filter((_, i) => i !== index));
  };

  const handleSubmit = async () => {
    setLoading(true);
    setError("");
    try {
      await apiClient.post("/v1/account/profile", {
        name: displayName.trim(),
        handles: handles.filter((h) => h.handle.trim()),
      });
      finishOnboarding();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const canSubmit = displayName.trim() && handles.some((h) => h.handle.trim());

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      minHeight: "100vh", background: "#f3f4f6", fontFamily: "system-ui",
    }}>
      <div style={{
        background: "#fff", padding: "2rem", borderRadius: "8px",
        width: "100%", maxWidth: "480px", boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
      }}>
        <h1 style={{ margin: "0 0 0.5rem", fontSize: "1.5rem", textAlign: "center" }}>
          Welcome to Message Vault
        </h1>
        <p style={{ textAlign: "center", color: "#6b7280", fontSize: "0.875rem", marginBottom: "1.5rem" }}>
          Set up your profile so we can match imported messages to you.
        </p>

        <label style={labelStyle}>Display Name</label>
        <input type="text" value={displayName} onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Your name" style={inputStyle} autoFocus />

        <label style={{ ...labelStyle, marginTop: "1rem" }}>My Handles</label>
        <p style={{ fontSize: "0.75rem", color: "#9ca3af", marginBottom: "0.5rem" }}>
          Add handles you use across services. These are used to match your messages.
        </p>

        {handles.map((h, i) => (
          <div key={i} style={{ display: "flex", gap: "0.5rem", marginBottom: "0.5rem" }}>
            <select value={h.service} onChange={(e) => updateHandle(i, "service", e.target.value)}
              style={{ padding: "0.375rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", width: "120px" }}>
              {SERVICES.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
            <input type="text" value={h.handle} onChange={(e) => updateHandle(i, "handle", e.target.value)}
              placeholder={h.service === "phone" ? "+1 555-1234" : h.service === "discord" ? "user#1234" : "@handle"}
              style={{ flex: 1, padding: "0.375rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px" }} />
            <button onClick={() => removeHandle(i)} disabled={handles.length === 1}
              style={{ border: "none", background: "none", color: "#9ca3af", cursor: "pointer", fontSize: "1.25rem" }}>
              ×
            </button>
          </div>
        ))}
        <button onClick={addHandle}
          style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", padding: 0 }}>
          + Add another handle
        </button>

        {error && (
          <div style={{ marginTop: "1rem", padding: "0.5rem 0.75rem", background: "#fef2f2", border: "1px solid #fecaca", borderRadius: "4px", color: "#991b1b", fontSize: "0.813rem" }}>
            {error}
          </div>
        )}

        <button onClick={handleSubmit} disabled={!canSubmit || loading}
          style={{ width: "100%", marginTop: "1.5rem", padding: "0.75rem", fontSize: "1rem", fontWeight: 600 }}>
          {loading ? "Saving…" : "Continue to Vault"}
        </button>
        <button
          onClick={logout}
          style={{
            width: "100%",
            padding: "0.5rem",
            fontSize: "0.875rem",
            marginTop: "0.5rem",
            background: "transparent",
            border: "none",
            color: "#9ca3af",
            cursor: "pointer",
          }}
        >
          Sign out
        </button>
      </div>
    </div>
  );
}

const labelStyle: React.CSSProperties = {
  fontSize: "0.875rem", fontWeight: 500, display: "block", marginBottom: "0.25rem",
};

const inputStyle: React.CSSProperties = {
  width: "100%", padding: "0.5rem", fontSize: "0.875rem",
  border: "1px solid #d1d5db", borderRadius: "4px", boxSizing: "border-box",
};
