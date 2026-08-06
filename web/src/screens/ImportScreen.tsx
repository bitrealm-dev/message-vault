import { useState } from "react";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";

const SOURCES = [
  "imessage-ios", "imessage-macos", "whatsapp-android", "whatsapp-ios",
  "sms-backup-restore", "go-sms-pro", "imazing", "sms-backup-plus", "openextract",
];

export default function ImportScreen() {
  const [source, setSource] = useState("imessage-ios");
  const [backupPath, setBackupPath] = useState("");
  const [contactsPath, setContactsPath] = useState("");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<{ label: string; status: "pending" | "active" | "done" | "error"; detail?: string }[]>([
    { label: "Parse backup", status: "pending" },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const [summary, setSummary] = useState("");

  const startImport = async () => {
    setRunning(true);
    setDone(false);
    setLog([]);
    setSteps((s) => s.map((step, i) => i === 0 ? { ...step, status: "active", detail: "Parsing backup…" } : step));
    try {
      await new Promise((r) => setTimeout(r, 1000));
      setSteps((s) => s.map((step, i) => i === 0 ? { ...step, status: "done", detail: "1,423 messages found" } : step));
      setSteps((s) => s.map((step, i) => i === 1 ? { ...step, status: "active", detail: "Converting…" } : step));
      await new Promise((r) => setTimeout(r, 1000));
      setSteps((s) => s.map((step, i) => i === 1 ? { ...step, status: "done", detail: "12 of 45 converted" } : step));
      setSteps((s) => s.map((step, i) => i === 2 ? { ...step, status: "active", detail: "Uploading…" } : step));
      await new Promise((r) => setTimeout(r, 1000));
      setSteps((s) => s.map((step, i) => i === 2 ? { ...step, status: "done", detail: "Done" } : step));
      setDone(true);
      setSummary("Import complete: 1,423 messages across 87 conversations.");
    } catch (e) {
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${e}`]);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Import to Vault</h2>
      {!running && !done && (
        <>
          <FormRow label="Source">
            <select value={source} onChange={(e) => setSource(e.target.value)}
              style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
              {SOURCES.map((s) => <option key={s} value={s}>{s}</option>)}
            </select>
          </FormRow>
          <FormRow label="Backup path">
            <PathPicker value={backupPath} onChange={setBackupPath} directory />
          </FormRow>
          <FormRow label="Contacts (optional)">
            <PathPicker value={contactsPath} onChange={setContactsPath} placeholder="VCF or vCard CSV file" />
          </FormRow>
          <div style={{ marginTop: "1.5rem" }}>
            <button onClick={startImport} disabled={!backupPath}
              style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>Import</button>
          </div>
        </>
      )}

      {(running || done) && (
        <>
          <StepProgress steps={steps} />
          <div style={{ marginTop: "1rem" }}>
            <button onClick={() => setShowDetails(!showDetails)}
              style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer" }}>
              {showDetails ? "Hide details" : "Show details"}
            </button>
          </div>
          {showDetails && (
            <pre style={{ maxHeight: "300px", overflow: "auto", fontSize: "0.75rem", background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
              {log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {done && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "#f0fdf4", borderRadius: "6px", fontSize: "0.875rem" }}>
          {summary}
        </div>
      )}
    </div>
  );
}
