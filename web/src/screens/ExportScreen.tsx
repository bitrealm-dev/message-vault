import { useState } from "react";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";

type ExportScope = "all" | "current-view" | "selected";
const FORMATS = ["jsonl", "json", "csv"];

export default function ExportScreen({ scope, selectedCount }: { scope: ExportScope; selectedCount: number }) {
  const [savePath, setSavePath] = useState("");
  const [format, setFormat] = useState("jsonl");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<{ label: string; status: "pending" | "active" | "done" | "error"; detail?: string }[]>([
    { label: "Exporting messages", status: "pending" },
    { label: "Writing attachments", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);

  const scopeLabel =
    scope === "all" ? "entire vault" :
    scope === "current-view" ? "current view" :
    `${selectedCount} conversation${selectedCount !== 1 ? "s" : ""}`;

  const startExport = async () => {
    setRunning(true);
    setDone(false);
    setSteps((s) => s.map((step, i) => i === 0 ? { ...step, status: "active" } : step));
    try {
      await new Promise((r) => setTimeout(r, 1500));
      setSteps((s) => s.map((step) => ({ ...step, status: "done" })));
      setDone(true);
    } catch (e) {
      setLog((l) => [...l, `Error: ${e}`]);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Export</h2>
      <p style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "1.5rem" }}>Exporting {scopeLabel}</p>

      <FormRow label="Save to">
        <PathPicker value={savePath} onChange={setSavePath} directory placeholder="Choose folder…" />
      </FormRow>
      <FormRow label="Format">
        <select value={format} onChange={(e) => setFormat(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }}>
          {FORMATS.map((f) => <option key={f} value={f}>{f.toUpperCase()}</option>)}
        </select>
      </FormRow>

      <div style={{ marginTop: "1.5rem" }}>
        <button onClick={startExport} disabled={running || !savePath}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
          {running ? "Exporting…" : "Export"}
        </button>
      </div>

      {(running || done) && (
        <>
          <StepProgress steps={steps} />
          <button onClick={() => setShowDetails(!showDetails)}
            style={{ fontSize: "0.813rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", marginTop: "0.5rem" }}>
            {showDetails ? "Hide details" : "Show details"}
          </button>
          {showDetails && (
            <pre style={{ maxHeight: "300px", overflow: "auto", fontSize: "0.75rem", background: "#f3f4f6", padding: "0.5rem", borderRadius: "4px", whiteSpace: "pre-wrap" }}>
              {log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {done && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "#f0fdf4", borderRadius: "6px", fontSize: "0.875rem" }}>
          Export complete. Files saved to {savePath}.
        </div>
      )}
    </div>
  );
}
