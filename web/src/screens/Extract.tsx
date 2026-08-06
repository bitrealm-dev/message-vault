import { useState, useCallback, useRef } from "react";
import { invokeExtract, invokeCancel, onExtractEvents } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import type { UnlistenFn } from "@tauri-apps/api/event";

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

const INDETERMINATE_KEYFRAMES = `
@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
`;

export default function Extract({ onError, onBack }: { onError?: (msg: string) => void; onBack?: () => void }) {
  const [source, setSource] = useState("sms-backup-restore");
  const [backupPath, setBackupPath] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(async () => {
    setRunning(true);
    setLog([]);

    unlistenRef.current = await onExtractEvents({
      onLog: (line) => {
        setLog((prev) => [...prev, line]);
      },
      onFinished: (summary) => {
        setLog((prev) => [...prev, summary]);
        setRunning(false);
      },
      onError: (err) => {
        setLog((prev) => [...prev, `Error: ${err.detail}`]);
        if (err.user_message) {
          setLog((prev) => [...prev, err.user_message!]);
        }
        setRunning(false);
        onError?.(err.user_message ?? err.detail);
      },
    });

    try {
      await invokeExtract({ source, path: backupPath, output_dir: outputDir });
    } catch (err) {
      setLog((prev) => [...prev, `Error starting extraction: ${err}`]);
      setRunning(false);
    }
  }, [source, backupPath, outputDir]);

  const cancel = useCallback(async () => {
    await invokeCancel();
  }, []);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <style>{INDETERMINATE_KEYFRAMES}</style>
      {onBack && (
        <button
          onClick={onBack}
          style={{
            marginBottom: "1rem", border: "none", background: "none",
            color: "#2563eb", cursor: "pointer", fontSize: "0.875rem", padding: 0,
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
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem", width: "100%" }}
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
        <button
          onClick={start}
          disabled={running || !backupPath || !outputDir}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}
        >
          {running ? "Running…" : "Extract"}
        </button>
        <button
          onClick={cancel}
          disabled={!running}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          Cancel
        </button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
