import { useState } from "react";
import { invokeExtract } from "../lib/tauri";
import { useTauriJob } from "../hooks/useTauriJob";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import Button from "../components/Button";

const SOURCES = [
  { id: "sms-backup-restore", label: "SMS Backup & Restore" },
  { id: "imessage-ios", label: "iMessage (iOS)" },
  { id: "imessage-macos", label: "iMessage (macOS)" },
  { id: "whatsapp-android", label: "WhatsApp (Android)" },
  { id: "whatsapp-ios", label: "WhatsApp (iOS)" },
  { id: "go-sms-pro", label: "GO SMS Pro" },
  { id: "imazing", label: "iMazing" },
  { id: "sms-backup-plus", label: "SMS Backup+" },
  { id: "openextract", label: "OpenExtract" },
];

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
        <select
          value={source}
          onChange={(e) => setSource(e.target.value)}
          style={{
            padding: "0.25rem 0.5rem",
            fontSize: "0.875rem",
            width: "100%",
            background: "var(--bg)",
            color: "var(--text)",
            border: "1px solid var(--border)",
            borderRadius: "4px",
          }}
        >
          {SOURCES.map((s) => (
            <option key={s.id} value={s.id}>
              {s.label}
            </option>
          ))}
        </select>
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
