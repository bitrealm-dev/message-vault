import { useState, useEffect } from "react";
import { isTauri } from "../lib/tauri-check";
import { apiClient } from "../lib/api";
import FormRow from "../components/FormRow";

export default function SettingsScreen() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [saved, setSaved] = useState(false);
  const [theme, setTheme] = useState(() => localStorage.getItem("mv-theme") || "system");

  const handleSave = () => {
    localStorage.setItem("mv-theme", theme);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Settings</h2>

      {isTauri() && (
        <>
          <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>Media</h3>
          <FormRow label="ffmpeg path">
            <input
              type="text"
              value={ffmpegPath}
              onChange={(e) => setFfmpegPath(e.target.value)}
              placeholder="Uses system PATH by default"
              style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}
            />
          </FormRow>
          <p style={{ fontSize: "0.75rem", color: "#9ca3af", marginTop: "0.25rem" }}>
            Leave blank to use system PATH.{" "}
            <a
              href="https://bitrealm-dev.github.io/message-vault-io/ffmpeg"
              target="_blank"
              rel="noopener"
              style={{ color: "#2563eb" }}
            >
              Install help
            </a>
          </p>
        </>
      )}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>
        Appearance
      </h3>
      <FormRow label="Theme">
        <select
          value={theme}
          onChange={(e) => setTheme(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}
        >
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </FormRow>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>
        Storage
      </h3>
      <StorageSection />

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <button onClick={handleSave} style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
          Save
        </button>
        {saved && <span style={{ fontSize: "0.875rem", color: "#16a34a" }}>Saved</span>}
      </div>
    </div>
  );
}

function StorageSection() {
  const [stats, setStats] = useState<{
    conversations: number;
    messages: number;
    attachments: number;
  } | null>(null);

  useEffect(() => {
    apiClient
      .get<{ conversations: number; messages: number; attachments: number }>(
        "/v1/export/messages/count?q="
      )
      .then((res) => setStats(res))
      .catch(() => {});
  }, []);

  if (!stats) return <div style={{ fontSize: "0.813rem", color: "#9ca3af" }}>Loading…</div>;

  return (
    <div style={{ fontSize: "0.875rem", color: "#374151" }}>
      <div>{stats.messages.toLocaleString()} messages</div>
      <div>{stats.conversations.toLocaleString()} conversations</div>
      <div>{stats.attachments.toLocaleString()} attachments</div>
    </div>
  );
}
