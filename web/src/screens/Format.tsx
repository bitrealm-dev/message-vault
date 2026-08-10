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
    <div className="max-w-[700px] p-6">
      {onBack && (
        <button
          onClick={onBack}
          className="mb-4 cursor-pointer border-none bg-none p-0 text-[0.875rem] text-accent"
        >
          ← Back to login
        </button>
      )}
      <h2 className="m-0 mb-6">Format Conversion</h2>

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

      <div className="mt-6 flex gap-3">
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
          className="!px-6 !py-2"
        >
          {running ? "Converting…" : "Convert"}
        </Button>
        <Button onClick={cancel} disabled={!running} className="!px-6 !py-2">
          Cancel
        </Button>
      </div>

      <div className="mt-6">
        <ProgressBar log={log} running={running} />
      </div>
    </div>
  );
}
