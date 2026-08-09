import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";

interface HandleInput {
  handle: string;
  service: string;
}

const SERVICES: { value: string; label: string }[] = [
  { value: "phone", label: "Phone Number" },
  { value: "email", label: "Email" },
  { value: "whatsapp", label: "WhatsApp" },
];

export default function OnboardingScreen() {
  const { login, logout, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>([
    { handle: "", service: "phone" },
  ]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const addHandle = () => {
    setHandles([...handles, { handle: "", service: "phone" }]);
  };

  const updateHandle = (
    index: number,
    field: keyof HandleInput,
    value: string,
  ) => {
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
        preferred_name: displayName.trim(),
        handles: handles
          .filter((h) => h.handle.trim())
          .map((h) => ({
            handle: h.handle.trim(),
            service: h.service,
          })),
      });
      // Re-run login so needsOnboarding is recomputed from the saved profile
      await login(serverUrl, token!, accountId!);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const canSubmit =
    displayName.trim() && handles.some((h) => h.handle.trim());

  return (
    <div style={pageStyle}>
      <div style={cardStyle}>
        <h1 style={titleStyle}>Profile Setup</h1>
        <p style={greetingStyle}>Welcome to the Message Vault!</p>
        <p style={bodyStyle}>
          Set up your profile so we can match imported message data to you.
        </p>

        <label style={labelStyle}>Display Name</label>
        <input
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Your name"
          style={inputStyle}
          autoFocus
        />

        <label style={{ ...labelStyle, marginTop: "1rem" }}>Source Accounts</label>
        <p style={helpStyle}>
          Add the accounts or phone numbers you import data from.
        </p>

        {handles.map((h, i) => (
          <div
            key={i}
            style={{ display: "flex", gap: "0.5rem", marginBottom: "0.5rem" }}
          >
            <select
              value={h.service}
              onChange={(e) => updateHandle(i, "service", e.target.value)}
              style={{
                padding: "0.375rem 0.5rem",
                fontSize: "0.875rem",
                border: "1px solid #d1d5db",
                borderRadius: "4px",
                width: "140px",
              }}
            >
              {SERVICES.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
            <input
              type="text"
              value={h.handle}
              onChange={(e) => updateHandle(i, "handle", e.target.value)}
              placeholder={
                h.service === "email"
                  ? "you@example.com"
                  : "+1 555-123-4567"
              }
              style={{
                flex: 1,
                padding: "0.375rem 0.5rem",
                fontSize: "0.875rem",
                border: "1px solid #d1d5db",
                borderRadius: "4px",
              }}
            />
            <button
              type="button"
              onClick={() => removeHandle(i)}
              disabled={handles.length === 1}
              style={{
                border: "none",
                background: "none",
                color: "#9ca3af",
                cursor: handles.length === 1 ? "default" : "pointer",
                fontSize: "1.25rem",
              }}
              aria-label="Remove account"
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={addHandle}
          style={{
            fontSize: "0.813rem",
            border: "none",
            background: "none",
            color: "#2563eb",
            cursor: "pointer",
            padding: 0,
          }}
        >
          + Add another account
        </button>

        <button
          onClick={handleSubmit}
          disabled={!canSubmit || loading}
          style={{
            width: "100%",
            marginTop: "1.5rem",
            padding: "0.75rem",
            fontSize: "1rem",
            fontWeight: 600,
          }}
        >
          {loading ? "Saving…" : "Continue to Vault"}
        </button>

        <button type="button" onClick={logout} style={backLinkStyle}>
          ← Back to login
        </button>

        <div
          style={{
            marginTop: "1.25rem",
            minHeight: "2.5rem",
            fontSize: "0.813rem",
            lineHeight: 1.35,
            color: error ? "#991b1b" : "transparent",
          }}
          aria-live="polite"
        >
          {error || "\u00a0"}
        </div>
      </div>
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "100vh",
  background: "#f3f4f6",
  fontFamily: "system-ui",
};

const cardStyle: React.CSSProperties = {
  background: "#fff",
  padding: "2rem",
  borderRadius: "8px",
  width: "100%",
  maxWidth: "480px",
  boxShadow: "0 1px 3px rgba(0, 0, 0, 0.1)",
};

const titleStyle: React.CSSProperties = {
  margin: "0 0 0.5rem",
  fontSize: "1.5rem",
  textAlign: "center",
};

const greetingStyle: React.CSSProperties = {
  textAlign: "center",
  color: "#374151",
  fontSize: "0.9375rem",
  margin: "0 0 0.5rem",
  fontWeight: 500,
};

const bodyStyle: React.CSSProperties = {
  textAlign: "center",
  color: "#6b7280",
  fontSize: "0.875rem",
  margin: "0 0 1.5rem",
};

const labelStyle: React.CSSProperties = {
  fontSize: "0.875rem",
  fontWeight: 500,
  display: "block",
  marginBottom: "0.25rem",
};

const helpStyle: React.CSSProperties = {
  fontSize: "0.75rem",
  color: "#9ca3af",
  marginBottom: "0.5rem",
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "0.5rem",
  fontSize: "0.875rem",
  border: "1px solid #d1d5db",
  borderRadius: "4px",
  boxSizing: "border-box",
};

const backLinkStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  marginTop: "0.75rem",
  padding: "0.25rem",
  fontSize: "0.875rem",
  background: "transparent",
  border: "none",
  color: "#4f46e5",
  textDecoration: "underline",
  cursor: "pointer",
  textAlign: "center",
};
