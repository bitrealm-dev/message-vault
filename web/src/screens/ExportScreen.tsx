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
      <div className="max-w-[700px] p-6 text-muted">
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
    <div className="max-w-[700px] p-6">
      <h2 className="m-0 mb-6">Export</h2>

      <p className="mb-6 text-[0.875rem] text-muted">
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

      <div className="mt-6 flex gap-3">
        <Button
          variant="primary"
          onClick={startExport}
          disabled={running || !savePath}
          className="!px-6 !py-2"
        >
          {running ? "Exporting…" : "Export"}
        </Button>
        <Button onClick={cancel} disabled={!running} className="!px-6 !py-2">
          Cancel
        </Button>
      </div>

      {error && (
        <div className="mt-4 rounded border border-danger-soft-border bg-danger-soft-bg p-3 text-[0.813rem] text-danger">
          {error}
        </div>
      )}

      <div className="mt-6">
        <ProgressBar log={log} running={running} />
      </div>

      {finished && (
        <div className="mt-4 rounded-md bg-ok-soft-bg p-4 text-[0.875rem]">
          Export complete. Files saved to {savePath}.
        </div>
      )}
    </div>
  );
}
