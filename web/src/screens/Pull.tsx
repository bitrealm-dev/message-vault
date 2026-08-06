import { useState, useCallback, useRef } from "react";
import { invokePull, invokeCancel, onExtractEvents } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import type { UnlistenFn } from "@tauri-apps/api/event";

const INDETERMINATE_KEYFRAMES = `
@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
`;

export default function Pull() {
  const [baseUrl, setBaseUrl] = useState("");
  const [username, setUsername] = useState("");
  const [key, setKey] = useState("");
  const [outDir, setOutDir] = useState("");
  const [query, setQuery] = useState("");
  const [skipAttachments, setSkipAttachments] = useState(false);
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(async () => {
    setRunning(true);
    setLog([]);

    unlistenRef.current = await onExtractEvents({
      onLog: (line) => setLog((prev) => [...prev, line]),
      onFinished: (summary) => {
        setLog((prev) => [...prev, summary]);
        setRunning(false);
      },
      onError: (err) => {
        setLog((prev) => [...prev, `Error: ${err.detail}`]);
        if (err.user_message) setLog((prev) => [...prev, err.user_message!]);
        setRunning(false);
      },
    });

    try {
      await invokePull({ base_url: baseUrl, username, key, out_dir: outDir, query, skip_attachments: skipAttachments });
    } catch (err) {
      setLog((prev) => [...prev, `Error starting pull: ${err}`]);
      setRunning(false);
    }
  }, [baseUrl, username, key, outDir, query, skipAttachments]);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <style>{INDETERMINATE_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Vault Pull</h2>

      <FormRow label="Server URL">
        <input type="text" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://vault.example.com" style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      <FormRow label="Username">
        <input type="text" value={username} onChange={(e) => setUsername(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      <FormRow label="API Key">
        <input type="password" value={key} onChange={(e) => setKey(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outDir} onChange={setOutDir} directory />
      </FormRow>

      <FormRow label="Search query">
        <input type="text" value={query} onChange={(e) => setQuery(e.target.value)}
          placeholder="e.g. from:alice before:2024-01-01" style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
      </FormRow>

      <FormRow label="Options">
        <label style={{ fontSize: "0.875rem" }}>
          <input type="checkbox" checked={skipAttachments} onChange={(e) => setSkipAttachments(e.target.checked)} /> Skip attachments
        </label>
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <button onClick={start} disabled={running || !baseUrl || !username || !key || !outDir}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}>
          {running ? "Pulling…" : "Pull"}
        </button>
        <button onClick={() => invokeCancel()} disabled={!running} style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
