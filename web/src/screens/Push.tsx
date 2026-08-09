import { useState, useCallback, useRef } from "react";
import { invokePush, invokeCancel, onExtractEvents } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import type { UnlistenFn } from "@tauri-apps/api/event";
import Button from "../components/Button";

const INDETERMINATE_KEYFRAMES = `
@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
`;

export default function Push({ onError }: { onError?: (msg: string) => void }) {
  const [baseUrl, setBaseUrl] = useState("");
  const [username, setUsername] = useState("");
  const [key, setKey] = useState("");
  const [inputDir, setInputDir] = useState("");
  const [mode, setMode] = useState("append");
  const [force, setForce] = useState(false);
  const [skipAttachments, setSkipAttachments] = useState(false);
  const [trustExport, setTrustExport] = useState(false);
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
        onError?.(err.user_message ?? err.detail);
      },
    });

    try {
      await invokePush({ base_url: baseUrl, username, key, input_dir: inputDir, mode, force, skip_attachments: skipAttachments, trust_export: trustExport });
    } catch (err) {
      setLog((prev) => [...prev, `Error starting push: ${err}`]);
      setRunning(false);
    }
  }, [baseUrl, username, key, inputDir, mode, force, skipAttachments, trustExport]);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <style>{INDETERMINATE_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Vault Push</h2>

      <FormRow label="Server URL">
        <input type="text" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://vault.example.com" style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "4px" }} />
      </FormRow>

      <FormRow label="Username">
        <input type="text" value={username} onChange={(e) => setUsername(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "4px" }} />
      </FormRow>

      <FormRow label="API Key">
        <input type="password" value={key} onChange={(e) => setKey(e.target.value)}
          style={{ width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.875rem", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "4px" }} />
      </FormRow>

      <FormRow label="Input directory">
        <PathPicker value={inputDir} onChange={setInputDir} directory placeholder="Extract output to push" />
      </FormRow>

      <FormRow label="Mode">
        <select value={mode} onChange={(e) => setMode(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem", width: "100%", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "4px" }}>
          <option value="append">Append</option>
          <option value="replace">Replace (with force)</option>
        </select>
      </FormRow>

      <FormRow label="Options">
        <label style={{ fontSize: "0.875rem", marginRight: "1rem" }}>
          <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} /> Force
        </label>
        <label style={{ fontSize: "0.875rem" }}>
          <input type="checkbox" checked={skipAttachments} onChange={(e) => setSkipAttachments(e.target.checked)} /> Skip attachments
        </label>
        <label style={{ fontSize: "0.875rem" }}>
          <input type="checkbox" checked={trustExport} onChange={(e) => setTrustExport(e.target.checked)} /> Trust export (skip hash verification)
        </label>
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <Button variant="primary" onClick={start} disabled={running || !baseUrl || !username || !key || !inputDir}
          style={{ padding: "0.5rem 1.5rem" }}>
          {running ? "Pushing…" : "Push"}
        </Button>
        <Button onClick={() => invokeCancel()} disabled={!running} style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </Button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
