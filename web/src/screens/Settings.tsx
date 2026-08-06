import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import FormRow from "../components/FormRow";

interface AppSettings {
  vault_url: string;
  vault_username: string;
  vault_key: string;
  default_output_dir: string;
}

export default function Settings() {
  const [vaultUrl, setVaultUrl] = useState("");
  const [vaultUsername, setVaultUsername] = useState("");
  const [vaultKey, setVaultKey] = useState("");
  const [defaultOutputDir, setDefaultOutputDir] = useState("");
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<AppSettings>("load_settings")
      .then((s) => {
        setVaultUrl(s.vault_url);
        setVaultUsername(s.vault_username);
        setVaultKey(s.vault_key);
        setDefaultOutputDir(s.default_output_dir);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  if (!loaded) {
    return <div style={{ padding: "1.5rem" }}>Loading settings…</div>;
  }

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Settings</h2>

      <h3 style={{ margin: "1.5rem 0 0.75rem", fontSize: "1rem", color: "#374151" }}>Vault Connection</h3>
      <FormRow label="Server URL">
        <input type="text" value={vaultUrl} readOnly
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "#f3f4f6" }} />
      </FormRow>
      <FormRow label="Username">
        <input type="text" value={vaultUsername} readOnly
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "#f3f4f6" }} />
      </FormRow>

      <h3 style={{ margin: "1.5rem 0 0.75rem", fontSize: "1rem", color: "#374151" }}>Defaults</h3>
      <FormRow label="Output directory">
        <input type="text" value={defaultOutputDir} readOnly
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "#f3f4f6" }} />
      </FormRow>

      <p style={{ marginTop: "1.5rem", fontSize: "0.8rem", color: "#9ca3af" }}>
        Settings are read from <code>export.ini</code> in the working directory.
        Edit the file directly or run an export to persist values.
      </p>
    </div>
  );
}
