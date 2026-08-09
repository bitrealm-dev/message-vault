import { useState, useEffect } from "react";
import { loadSettings, saveSettings, type AppSettings } from "../lib/tauri";
import FormRow from "../components/FormRow";

/** Legacy Tauri settings page (defaults only). Vault connection lives in login/auth. */
export default function Settings() {
  const [settings, setSettings] = useState<AppSettings>({
    vault_url: "",
    vault_username: "",
    vault_key: "",
    default_output_dir: "",
  });
  const [loaded, setLoaded] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    loadSettings()
      .then((s) => setSettings(s))
      .catch(() => {})
      .finally(() => setLoaded(true));
  }, []);

  const handleSave = async () => {
    try {
      await saveSettings(settings);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch {
      // save failed — state stays editable
    }
  };

  if (!loaded) {
    return <div style={{ padding: "1.5rem" }}>Loading settings…</div>;
  }

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Settings</h2>

      <h3 style={{ margin: "1.5rem 0 0.75rem", fontSize: "1rem", color: "#374151" }}>
        Defaults
      </h3>
      <FormRow label="Output directory">
        <input
          type="text"
          value={settings.default_output_dir}
          onChange={(e) => setSettings({ ...settings, default_output_dir: e.target.value })}
          placeholder="Default output path for exports"
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}
        />
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <button
          onClick={handleSave}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}
        >
          Save
        </button>
        {saved && (
          <span style={{ fontSize: "0.875rem", color: "#16a34a" }}>
            Saved to export.ini
          </span>
        )}
      </div>

      <p style={{ marginTop: "1.5rem", fontSize: "0.8rem", color: "#9ca3af" }}>
        Settings persist in <code>export.ini</code> in the working directory.
      </p>
    </div>
  );
}
