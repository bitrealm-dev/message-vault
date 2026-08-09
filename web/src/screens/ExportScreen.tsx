import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import StepProgress from "../components/StepProgress";
import Button from "../components/Button";

const FORMATS = ["jsonl", "json", "csv"];

interface ExportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function ExportScreen() {
  const { token } = useAuth();
  const [savePath, setSavePath] = useState("");
  const [format, setFormat] = useState("jsonl");
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ExportStep[]>([
    { label: "Exporting messages", status: "pending" },
    { label: "Writing attachments", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [done, setDone] = useState(false);
  const [error, setError] = useState("");

  const startExport = async () => {
    if (!token) {
      setError("Not authenticated");
      return;
    }
    setRunning(true);
    setDone(false);
    setError("");
    setLog([]);

    setSteps((s) => s.map((step, i) =>
      i === 0 ? { ...step, status: "active", detail: "Fetching messages…" } : step
    ));

    try {
      if (isTauri()) {
        const { invokePull } = await import("../lib/tauri");
        const baseUrl = getBaseUrl();
        await invokePull({
          base_url: baseUrl,
          username: "",
          key: token,
          out_dir: savePath,
          query: "",
          skip_attachments: false,
        });
      } else {
        // Web fallback: fetch messages via API and trigger browser download
        const res = await apiClient.get<{ messages: unknown[]; total: number }>(
          `/v1/export/messages?q=&offset=0&limit=10000`,
        );

        let content: string;
        if (format === "jsonl") {
          content = (res.messages as Array<Record<string, unknown>>)
            .map((m) => JSON.stringify(m))
            .join("\n");
        } else if (format === "csv") {
          const msgs = res.messages as Array<Record<string, unknown>>;
          if (msgs.length === 0) {
            content = "";
          } else {
            const headers = Object.keys(msgs[0]).join(",");
            const rows = msgs.map((m) => Object.values(m).map((v) =>
              typeof v === "string" ? `"${v.replace(/"/g, '""')}"` : String(v ?? "")
            ).join(","));
            content = [headers, ...rows].join("\n");
          }
        } else {
          content = JSON.stringify(res.messages, null, 2);
        }

        const blob = new Blob([content], { type: "application/octet-stream" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `export.${format}`;
        a.click();
        URL.revokeObjectURL(url);
      }

      setSteps((s) => s.map((step) => ({ ...step, status: "done" as const })));
      setDone(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${msg}`]);
      setError(msg);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Export</h2>

      <p style={{ fontSize: "0.875rem", color: "var(--muted)", marginBottom: "1.5rem" }}>
        Exporting entire vault
      </p>

      <FormRow label="Save to">
        <PathPicker value={savePath} onChange={setSavePath} directory placeholder="Choose folder…" />
      </FormRow>

      <FormRow label="Format">
        <select value={format} onChange={(e) => setFormat(e.target.value)}
          style={{
            width: "100%",
            padding: "0.25rem 0.5rem",
            fontSize: "0.875rem",
            background: "var(--bg)",
            color: "var(--text)",
            border: "1px solid var(--border)",
            borderRadius: "4px",
          }}>
          {FORMATS.map((f) => <option key={f} value={f}>{f.toUpperCase()}</option>)}
        </select>
      </FormRow>

      <div style={{ marginTop: "1.5rem" }}>
        <Button variant="primary" onClick={startExport} disabled={running || !savePath}
          style={{ padding: "0.5rem 1.5rem" }}>
          {running ? "Exporting…" : "Export"}
        </Button>
      </div>

      {error && (
        <div style={{ marginTop: "1rem", padding: "0.75rem", background: "var(--danger-soft-bg)", border: "1px solid var(--danger-soft-border)", borderRadius: "4px", color: "var(--danger)", fontSize: "0.813rem" }}>
          {error}
        </div>
      )}

      {(running || done) && (
        <>
          <StepProgress steps={steps} />
          <button onClick={() => setShowDetails(!showDetails)}
            style={{ fontSize: "0.813rem", border: "none", background: "none", color: "var(--accent)", cursor: "pointer", marginTop: "0.5rem" }}>
            {showDetails ? "Hide details" : "Show details"}
          </button>
          {showDetails && (
            <pre style={{
              maxHeight: "300px", overflow: "auto", fontSize: "0.75rem",
              background: "var(--hover)", padding: "0.5rem", borderRadius: "4px",
              whiteSpace: "pre-wrap",
            }}>
              {log.length === 0 ? "No log entries" : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {done && (
        <div style={{ marginTop: "1rem", padding: "1rem", background: "var(--ok-soft-bg)", borderRadius: "6px", fontSize: "0.875rem" }}>
          Export complete. Files saved to {savePath}.
        </div>
      )}
    </div>
  );
}
