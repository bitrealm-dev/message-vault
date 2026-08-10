import { useState } from "react";
import { invokeExtract } from "../lib/tauri";
import { EXPORT_SOURCES } from "../lib/exportSources";
import { useTauriJob } from "../hooks/useTauriJob";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";

export default function Extract({
  onError,
  onBack,
}: {
  onError?: (msg: string) => void;
  onBack?: () => void;
}) {
  const [source, setSource] = useState("sms-backup-restore");
  const [backupPath, setBackupPath] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const { running, log, start, cancel } = useTauriJob({ onError });

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      {onBack && (
        <button
          onClick={onBack}
          style={{
            marginBottom: "1rem",
            border: "none",
            background: "none",
            color: "var(--accent)",
            cursor: "pointer",
            fontSize: "0.875rem",
            padding: 0,
          }}
        >
          ← Back to login
        </button>
      )}
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Extract Messages</h2>

      <FormRow label="Source">
        <Select
          selectedKey={source}
          onSelectionChange={(k) => setSource(String(k))}
          aria-label="Source"
          triggerClassName="!bg-bg"
        >
          {EXPORT_SOURCES.map((s) => (
            <ListBoxItem key={s.id} id={s.id} className={selectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
      </FormRow>

      <FormRow label="Backup path">
        <PathPicker value={backupPath} onChange={setBackupPath} directory />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outputDir} onChange={setOutputDir} directory />
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <Button
          variant="primary"
          onClick={() =>
            start(
              () =>
                invokeExtract({
                  source,
                  path: backupPath,
                  output_dir: outputDir,
                }),
              "Error starting extraction",
            )
          }
          disabled={running || !backupPath || !outputDir}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          {running ? "Running…" : "Extract"}
        </Button>
        <Button onClick={cancel} disabled={!running} style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </Button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
