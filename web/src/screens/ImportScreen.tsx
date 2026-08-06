import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { invokeExtract, invokeContactsInfo, type ContactCard } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";
import ContactReviewTable from "../components/ContactReviewTable";

const SOURCES = [
  "imessage-ios", "imessage-macos", "whatsapp-android", "whatsapp-ios",
  "sms-backup-restore", "go-sms-pro", "imazing", "sms-backup-plus", "openextract",
];

interface ImportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function ImportScreen() {
  const { token } = useAuth();
  const [source, setSource] = useState("imessage-ios");
  const [backupPath, setBackupPath] = useState("");
  const [contactsPath, setContactsPath] = useState("");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>([
    { label: "Parse backup", status: "pending" },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const [summary, setSummary] = useState("");
  const [phase, setPhase] = useState<"form" | "contacts-review" | "progress" | "done">("form");
  const [fileCards, setFileCards] = useState<ContactCard[]>([]);

  const startImport = async () => {
    if (!isTauri()) return;
    setRunning(true);
    setPhase("progress");
    setDone(false);
    setLog([]);

    // Step 1: Parse backup
    setSteps((s) => s.map((step, i) =>
      i === 0 ? { ...step, status: "active", detail: "Parsing backup…" } : step
    ));

    try {
      // Run Tauri extract command — produces JSONL in a temp directory
      const outputDir = `${backupPath}/../extract-output`;
      await invokeExtract({ source, path: backupPath, output_dir: outputDir });

      setSteps((s) => s.map((step, i) =>
        i === 0 ? { ...step, status: "done", detail: "Extraction complete" } : step
      ));

      // Step 2: Convert attachments
      setSteps((s) => s.map((step, i) =>
        i === 1 ? { ...step, status: "active", detail: "Processing attachments…" } : step
      ));
      setSteps((s) => s.map((step, i) =>
        i === 1 ? { ...step, status: "done", detail: "Attachments processed" } : step
      ));

      // Step 3: Upload to vault
      setSteps((s) => s.map((step, i) =>
        i === 2 ? { ...step, status: "active", detail: "Uploading to vault…" } : step
      ));

      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      // Start an import session
      const importSession = await apiClient.post<{ id: string }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "push",
      });

      // Call the existing Tauri push command which handles the JSONL upload:
      const { invokePush } = await import("../lib/tauri");
      await invokePush({
        base_url: baseUrl,
        username: "",
        key: token,
        input_dir: outputDir,
        mode: "import",
        force: false,
        skip_attachments: false,
        trust_export: false,
      });

      // Complete the import session
      await apiClient.post(`/v1/imports/${importSession.id}/complete`, {});

      setSteps((s) => s.map((step, i) =>
        i === 2 ? { ...step, status: "done", detail: "Upload complete" } : step
      ));

      setPhase("done");
      setSummary("Import complete. Messages uploaded to vault.");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${msg}`]);
      setPhase("progress");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Import to Vault</h2>

      {phase === "form" && (
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

          <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
            <button onClick={startImport} disabled={!backupPath}
              style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
              Import
            </button>
            {contactsPath && isTauri() && (
              <button onClick={async () => {
                try {
                  const info = await invokeContactsInfo(contactsPath);
                  setFileCards(info.cards);
                  setPhase("contacts-review");
                } catch (e) { /* contacts parse failed — skip */ }
              }}
                style={{ padding: "0.5rem 1.5rem", fontSize: "0.875rem" }}>
                Review contacts
              </button>
            )}
          </div>
        </>
      )}

      {phase === "contacts-review" && (
        <ContactReviewTable
          fileCards={fileCards}
          onClose={() => setPhase("form")}
        />
      )}

      {(phase === "progress" || phase === "done") && (
        <>
          <StepProgress steps={steps} />
          <div style={{ marginTop: "1rem" }}>
            <button onClick={() => setShowDetails(!showDetails)}
              style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer" }}>
              {showDetails ? "Hide details" : "Show details"}
            </button>
          </div>
          {showDetails && (
            <pre style={{
              maxHeight: "300px", overflow: "auto", fontSize: "0.75rem",
              background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px",
              whiteSpace: "pre-wrap", wordBreak: "break-word",
            }}>
              {log.length === 0 ? "No log entries" : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {phase === "done" && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "#f0fdf4", borderRadius: "6px", fontSize: "0.875rem" }}>
          {summary}
        </div>
      )}

      {phase === "done" && (
        <div style={{ marginTop: "1rem" }}>
          <button onClick={() => { setPhase("form"); setDone(false); }}
            style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
            Import another
          </button>
        </div>
      )}
    </div>
  );
}
