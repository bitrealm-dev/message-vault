import { useState } from "react";
import { useAuth } from "../lib/auth";
import { getBaseUrl } from "../lib/api";
import { invokePull } from "../lib/tauri";
import { useTauriJob } from "../hooks/useTauriJob";
import TauriJobFormShell from "../components/TauriJobFormShell";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";

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
    <TauriJobFormShell
      title="Export"
      requireTauri
      startLabel="Export"
      runningLabel="Exporting…"
      running={running}
      log={log}
      startDisabled={!savePath}
      onStart={startExport}
      onCancel={cancel}
      error={error}
      intro={
        <p className="mb-6 text-[0.875rem] text-muted">
          Export the entire vault as JSONL (plus attachments) into a folder.
        </p>
      }
      success={
        finished ? (
          <div className="mt-4 rounded-md bg-ok-soft-bg p-4 text-[0.875rem]">
            Export complete. Files saved to {savePath}.
          </div>
        ) : null
      }
    >
      <FormRow label="Save to">
        <PathPicker
          value={savePath}
          onChange={setSavePath}
          directory
          placeholder="Choose folder…"
        />
      </FormRow>
    </TauriJobFormShell>
  );
}
