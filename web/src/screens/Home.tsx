import { useState, useEffect } from "react";
import { loadSettings, type AppSettings } from "../lib/tauri";

interface QuickAction {
  id: string;
  label: string;
  description: string;
  color: string;
}

const ACTIONS: QuickAction[] = [
  {
    id: "extract",
    label: "Extract Messages",
    description: "Pull conversations from a phone backup and export them as JSONL",
    color: "#2563eb",
  },
  {
    id: "format",
    label: "Format Conversion",
    description: "Convert an existing extract to EML, MBOX, CSV, or XML",
    color: "#7c3aed",
  },
  {
    id: "push",
    label: "Vault Push",
    description: "Import messages into a Message Vault server",
    color: "#059669",
  },
  {
    id: "pull",
    label: "Vault Pull",
    description: "Export messages from a Message Vault server",
    color: "#d97706",
  },
];

export default function Home({ onNavigate }: { onNavigate: (tab: string) => void }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);

  useEffect(() => {
    loadSettings()
      .then((s) => setSettings(s))
      .catch(() => {});
  }, []);

  const vaultConfigured = !!(settings?.vault_url && settings?.vault_username && settings?.vault_key);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 0.5rem 0", fontSize: "1.5rem" }}>Message Vault</h2>
      <p style={{ margin: "0 0 2rem 0", color: "#6b7280", fontSize: "0.875rem" }}>
        Extract, convert, and manage message backups
      </p>

      <div
        style={{
          background: vaultConfigured ? "#f0fdf4" : "#fefce8",
          border: vaultConfigured ? "1px solid #bbf7d0" : "1px solid #fef08a",
          borderRadius: "8px",
          padding: "0.75rem 1rem",
          marginBottom: "2rem",
          fontSize: "0.875rem",
        }}
      >
        <span style={{ fontWeight: 600, color: vaultConfigured ? "#166534" : "#854d0e" }}>
          Vault:{" "}
        </span>
        {vaultConfigured ? (
          <span style={{ color: "#166534" }}>
            Connected to {settings!.vault_url} as {settings!.vault_username}
          </span>
        ) : (
          <span style={{ color: "#854d0e" }}>
            Not configured — set up in Settings before using Push or Pull
          </span>
        )}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {ACTIONS.map((action) => (
          <button
            key={action.id}
            onClick={() => onNavigate(action.id)}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "1rem",
              padding: "1rem 1.25rem",
              border: "1px solid #e5e7eb",
              borderRadius: "8px",
              background: "#fff",
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            <div
              style={{
                width: "3px",
                height: "40px",
                borderRadius: "2px",
                background: action.color,
                flexShrink: 0,
              }}
            />
            <div>
              <div style={{ fontWeight: 600, fontSize: "0.938rem", marginBottom: "2px" }}>
                {action.label}
              </div>
              <div style={{ color: "#6b7280", fontSize: "0.813rem" }}>
                {action.description}
              </div>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
