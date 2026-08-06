import { useState, useCallback, useRef } from "react";
import { invokeFormat, invokeCancel, onExtractEvents } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import type { UnlistenFn } from "@tauri-apps/api/event";

const FORMATS = [
  { id: "json", label: "JSON" },
  { id: "jsonl", label: "JSONL" },
  { id: "csv", label: "CSV" },
  { id: "eml", label: "EML" },
  { id: "mbox", label: "MBOX" },
  { id: "xml", label: "XML (smses.xml)" },
];

const INDETERMINATE_KEYFRAMES = `
@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
`;

export default function Format() {
  const [inputDir, setInputDir] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [outputFormat, setOutputFormat] = useState("jsonl");
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
      await invokeFormat({ input_dir: inputDir, output_dir: outputDir, output_format: outputFormat });
    } catch (err) {
      setLog((prev) => [...prev, `Error starting format: ${err}`]);
      setRunning(false);
    }
  }, [inputDir, outputDir, outputFormat]);

  const cancel = useCallback(async () => {
    await invokeCancel();
  }, []);

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <style>{INDETERMINATE_KEYFRAMES}</style>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Format Conversion</h2>

      <FormRow label="Input directory">
        <PathPicker value={inputDir} onChange={setInputDir} directory placeholder="Previous extract output" />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outputDir} onChange={setOutputDir} directory />
      </FormRow>

      <FormRow label="Output format">
        <select
          value={outputFormat}
          onChange={(e) => setOutputFormat(e.target.value)}
          style={{ padding: "0.25rem 0.5rem", fontSize: "0.875rem", width: "100%" }}
        >
          {FORMATS.map((f) => (
            <option key={f.id} value={f.id}>{f.label}</option>
          ))}
        </select>
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <button
          onClick={start}
          disabled={running || !inputDir || !outputDir}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}
        >
          {running ? "Converting…" : "Convert"}
        </button>
        <button onClick={cancel} disabled={!running} style={{ padding: "0.5rem 1.5rem" }}>
          Cancel
        </button>
      </div>

      <div style={{ marginTop: "1.5rem" }}>
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
