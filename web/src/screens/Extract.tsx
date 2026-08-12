import { useState } from "react";
import { invokeExtract } from "../lib/tauri";
import { EXPORT_SOURCES } from "../lib/exportSources";
import { useTauriJob } from "../hooks/useTauriJob";
import BackToLoginLink from "../components/BackToLoginLink";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import ProgressBar from "../components/ProgressBar";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";

export default function Extract({
  onError,
  onBack,
}: {
  onError?: (msg: string) => void;
  onBack?: () => void;
}) {
  const [source, setSource] = useState("sms-backup-restore");
  const [backupPath, setBackupPath] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const { running, log, start, cancel } = useTauriJob({ onError });

  return (
    <div className="max-w-[700px] p-6">
      <BackToLoginLink onBack={onBack} />
      <h2 className="m-0 mb-6">Extract Messages</h2>

      <FormRow label="Source">
        <Select
          selectedKey={source}
          onSelectionChange={(k) => setSource(String(k))}
          aria-label="Source"
          triggerClassName="!bg-bg"
        >
          {EXPORT_SOURCES.map((s) => (
            <ListBoxItem key={s.id} id={s.id} className={selectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
      </FormRow>

      <FormRow label="Backup path">
        <PathPicker value={backupPath} onChange={setBackupPath} directory />
      </FormRow>

      <FormRow label="Output directory">
        <PathPicker value={outputDir} onChange={setOutputDir} directory />
      </FormRow>

      <div className="mt-6 flex gap-3">
        <Button
          variant="primary"
          onClick={() =>
            start(
              () =>
                invokeExtract({
                  source,
                  path: backupPath,
                  output_dir: outputDir,
                }),
              "Error starting extraction",
            )
          }
          disabled={running || !backupPath || !outputDir}
          className="!px-6 !py-2"
        >
          {running ? "Running…" : "Extract"}
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
