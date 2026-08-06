import { useState, useEffect } from "react";
import { loadSettings, saveSettings, type AppSettings } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import { apiClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import FormRow from "../components/FormRow";

export default function SettingsScreen() {
  const [settings, setSettings] = useState<AppSettings>({
    vault_url: "", vault_username: "", vault_key: "", default_output_dir: "",
  });
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState(false);
  const [theme, setTheme] = useState("system");

  useEffect(() => {
    if (isTauri()) {
      loadSettings().then(setSettings).catch(() => {}).finally(() => setLoaded(true));
    } else {
      setLoaded(true);
    }
  }, []);

  const handleSave = async () => {
    try {
      if (isTauri()) await saveSettings(settings);
      localStorage.setItem("mv-theme", theme);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch { /* */ }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Settings</h2>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>Vault Connection</h3>
      <FormRow label="Server URL">
        <input type="text" value={settings.vault_url}
          onChange={(e) => setSettings({ ...settings, vault_url: e.target.value })}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      {isTauri() && (
        <>
          <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Media</h3>
          <FormRow label="ffmpeg path">
            <input type="text" value={ffmpegPath}
              onChange={(e) => setFfmpegPath(e.target.value)}
              placeholder="Uses system PATH by default"
              style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
          </FormRow>
          <p style={{ fontSize: "0.75rem", color: "#9ca3af", marginTop: "0.25rem" }}>
            Leave blank to use system PATH.{" "}
            <a href="https://bitrealm-dev.github.io/message-vault-io/ffmpeg" target="_blank" rel="noopener" style={{ color: "#2563eb" }}>
              Install help
            </a>
          </p>
        </>
      )}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Appearance</h3>
      <FormRow label="Theme">
        <select value={theme} onChange={(e) => setTheme(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </FormRow>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Storage</h3>
      <StorageSection />
      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Account</h3>
      <AccountSection />

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <button onClick={handleSave} style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>Save</button>
        {saved && <span style={{ fontSize: "0.875rem", color: "#16a34a" }}>Saved</span>}
      </div>
    </div>
  );
}

function StorageSection() {
  const [stats, setStats] = useState<{ conversations: number; messages: number; attachments: number } | null>(null);

  useEffect(() => {
    apiClient
      .get<{ conversations: number; messages: number; attachments: number }>("/v1/export/messages/count?q=")
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

function AccountSection() {
  const { logout } = useAuth();
  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [pwMsg, setPwMsg] = useState("");

  const handleChangePassword = async () => {
    try {
      await apiClient.post("/v1/auth/change-password", {
        current_password: currentPw,
        new_password: newPw,
      });
      setPwMsg("Password changed.");
      setCurrentPw(""); setNewPw("");
    } catch (e) { setPwMsg(String(e)); }
  };

  const handleDeleteAccount = async () => {
    if (!confirm("Permanently delete your account and all data? This cannot be undone.")) return;
    try {
      await apiClient.post("/v1/auth/delete-account", { confirm: true });
      logout();
    } catch (e) { alert(String(e)); }
  };

  return (
    <div>
      <div style={{ marginBottom: "0.75rem" }}>
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>Current password</label>
        <input type="password" value={currentPw} onChange={(e) => setCurrentPw(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "0.5rem" }} />
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>New password</label>
        <input type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", border: "1px solid #d1d5db", borderRadius: "4px", marginBottom: "0.5rem" }} />
        <button onClick={handleChangePassword} disabled={!currentPw || !newPw}
          style={{ padding: "0.375rem 0.75rem", fontSize: "0.875rem" }}>Change password</button>
        {pwMsg && <div style={{ marginTop: "0.375rem", fontSize: "0.813rem", color: pwMsg.includes("changed") ? "#16a34a" : "#dc2626" }}>{pwMsg}</div>}
      </div>
      <div style={{ paddingTop: "0.75rem", borderTop: "1px solid #e5e7eb" }}>
        <button onClick={handleDeleteAccount}
          style={{ color: "#dc2626", border: "1px solid #fecaca", background: "#fef2f2", padding: "0.5rem 1rem", borderRadius: "4px", cursor: "pointer", fontSize: "0.875rem" }}>
          Delete account
        </button>
      </div>
    </div>
  );
}
