import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";
import {
  accentLink,
  authCard,
  authInput,
  authLabel,
  authTitle,
  mutedText,
  pageCenter,
} from "../lib/uiStyles";
import AuthSubmitButton from "../components/AuthSubmitButton";

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
    <div style={pageCenter}>
      <div style={{ ...authCard, maxWidth: "480px" }}>
        <h1 style={{ ...authTitle, marginBottom: "0.5rem" }}>Profile Setup</h1>
        <p style={greetingStyle}>Welcome to the Message Vault!</p>
        <p style={bodyStyle}>
          Set up your profile so we can match imported message data to you.
        </p>

        <label style={authLabel}>Display Name</label>
        <input
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Your name"
          style={authInput}
          autoFocus
        />

        <label style={{ ...authLabel, marginTop: "1rem" }}>Source Accounts</label>
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
                ...authInput,
                width: "140px",
                padding: "0.375rem 0.5rem",
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
                ...authInput,
                flex: 1,
                width: "auto",
                padding: "0.375rem 0.5rem",
              }}
            />
            <button
              type="button"
              onClick={() => removeHandle(i)}
              disabled={handles.length === 1}
              style={{
                border: "none",
                background: "none",
                color: "var(--muted)",
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
            color: "var(--accent)",
            cursor: "pointer",
            padding: 0,
          }}
        >
          + Add another account
        </button>

        <AuthSubmitButton
          onClick={handleSubmit}
          disabled={!canSubmit || loading}
          style={!canSubmit && !loading ? { filter: "brightness(0.72)" } : undefined}
        >
          {loading ? "Saving…" : "Continue to Vault"}
        </AuthSubmitButton>

        <button type="button" onClick={logout} style={{ ...accentLink, marginTop: "0.75rem" }}>
          ← Back to login
        </button>

        <div
          style={{
            marginTop: "1.25rem",
            minHeight: "2.5rem",
            fontSize: "0.813rem",
            lineHeight: 1.35,
            color: error ? "var(--danger)" : "transparent",
          }}
          aria-live="polite"
        >
          {error || "\u00a0"}
        </div>
      </div>
    </div>
  );
}

const greetingStyle: React.CSSProperties = {
  textAlign: "center",
  color: "var(--text)",
  fontSize: "0.9375rem",
  margin: "0 0 0.5rem",
  fontWeight: 500,
};

const bodyStyle: React.CSSProperties = {
  ...mutedText,
  textAlign: "center",
  fontSize: "0.875rem",
  margin: "0 0 1.5rem",
};

const helpStyle: React.CSSProperties = {
  ...mutedText,
  fontSize: "0.75rem",
  marginBottom: "0.5rem",
};
