import { useState } from "react";
import { invokeFormat } from "../lib/tauri";
import { useTauriJob } from "../hooks/useTauriJob";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";

const FORMATS = [
  { id: "json", label: "JSON" },
  { id: "jsonl", label: "JSONL" },
  { id: "csv", label: "CSV" },
  { id: "eml", label: "EML" },
  { id: "mbox", label: "MBOX" },
  { id: "xml", label: "XML (smses.xml)" },
];

export default function Format({
  onError,
  onBack,
}: {
  onError?: (msg: string) => void;
  onBack?: () => void;
}) {
  const [inputDir, setInputDir] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [outputFormat, setOutputFormat] = useState("jsonl");
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
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Format Conversion</h2>

      <FormRow label="Input directory">
        <PathPicker
          value={inputDir}
          onChange={setInputDir}
          directory
          placeholder="Previous extract output"
        />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outputDir} onChange={setOutputDir} directory />
      </FormRow>

      <FormRow label="Output format">
        <Select
          selectedKey={outputFormat}
          onSelectionChange={(k) => setOutputFormat(String(k))}
          aria-label="Output format"
          triggerClassName="!bg-bg"
        >
          {FORMATS.map((f) => (
            <ListBoxItem key={f.id} id={f.id} className={selectItemClassName}>
              {f.label}
            </ListBoxItem>
          ))}
        </Select>
      </FormRow>

      <div style={{ marginTop: "1.5rem", display: "flex", gap: "0.75rem" }}>
        <Button
          variant="primary"
          onClick={() =>
            start(
              () =>
                invokeFormat({
                  input_dir: inputDir,
                  output_dir: outputDir,
                  output_format: outputFormat,
                }),
              "Error starting format",
            )
          }
          disabled={running || !inputDir || !outputDir}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          {running ? "Converting…" : "Convert"}
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
