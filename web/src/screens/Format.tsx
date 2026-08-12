import { useState } from "react";
import { invokeFormat } from "../lib/tauri";
import { useTauriJob } from "../hooks/useTauriJob";
import TauriJobFormShell from "../components/TauriJobFormShell";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
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
    <TauriJobFormShell
      title="Format Conversion"
      onBack={onBack}
      startLabel="Convert"
      runningLabel="Converting…"
      running={running}
      log={log}
      startDisabled={!inputDir || !outputDir}
      onStart={() =>
        void start(
          () =>
            invokeFormat({
              input_dir: inputDir,
              output_dir: outputDir,
              output_format: outputFormat,
            }),
          "Error starting format",
        )
      }
      onCancel={cancel}
    >
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
    </TauriJobFormShell>
  );
}
