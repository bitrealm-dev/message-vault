import { useState } from "react";
import { useAuth } from "../lib/auth";
import { getBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";
import { invokePull } from "../lib/tauri";
import { useTauriJob } from "../hooks/useTauriJob";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import Button from "../components/Button";

/**
 * Desktop export via vault-pull. Always writes JSONL (and attachments)
 * into the chosen folder — the same format the Pull CLI produces.
 * Shown only when Tauri is available (see LeftPanel).
 */
export default function ExportScreen() {
  const { token } = useAuth();
  const [savePath, setSavePath] = useState("");
  const [error, setError] = useState("");
  const { running, finished, log, start, cancel } = useTauriJob({
    onError: (msg) => setError(msg),
  });

  if (!isTauri()) {
    return (
      <div style={{ padding: "1.5rem", maxWidth: "700px", color: "var(--muted)" }}>
        Export requires the desktop app.
      </div>
    );
  }

  const startExport = () => {
    if (!token) {
      setError("Not authenticated");
      return;
    }
    setError("");
    void start(
      () =>
        invokePull({
          base_url: getBaseUrl(),
          username: "",
          key: token,
          out_dir: savePath,
          query: "",
          skip_attachments: false,
        }),
      "Error starting export",
    );
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Export</h2>

      <p style={{ fontSize: "0.875rem", color: "var(--muted)", marginBottom: "1.5rem" }}>
        Export the entire vault as JSONL (plus attachments) into a folder.
      </p>

      <FormRow label="Save to">
        <PathPicker
          value={savePath}
          onChange={setSavePath}
          directory
          placeholder="Choose folder…"
        />
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <Button
          variant="primary"
          onClick={startExport}
          disabled={running || !savePath}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          {running ? "Exporting…" : "Export"}
        </Button>
        <Button onClick={cancel} disabled={!running} style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </Button>
      </div>

      {error && (
        <div
          style={{
            marginTop: "1rem",
            padding: "0.75rem",
            background: "var(--danger-soft-bg)",
            border: "1px solid var(--danger-soft-border)",
            borderRadius: "4px",
            color: "var(--danger)",
            fontSize: "0.813rem",
          }}
        >
          {error}
        </div>
      )}

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>

      {finished && (
        <div
          style={{
            marginTop: "1rem",
            padding: "1rem",
            background: "var(--ok-soft-bg)",
            borderRadius: "6px",
            fontSize: "0.875rem",
          }}
        >
          Export complete. Files saved to {savePath}.
        </div>
      )}
    </div>
  );
}
