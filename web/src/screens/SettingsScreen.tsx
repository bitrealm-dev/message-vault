import { useState, useEffect } from "react";
import { loadSettings, saveSettings, type AppSettings } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
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

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <button onClick={handleSave} style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>Save</button>
        {saved && <span style={{ fontSize: "0.875rem", color: "#16a34a" }}>Saved</span>}
      </div>
    </div>
  );
}
