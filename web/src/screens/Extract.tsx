import { useState } from "react";
import { invokeExtract } from "../lib/tauri";
import { EXPORT_SOURCES } from "../lib/exportSources";
import { useTauriJob } from "../hooks/useTauriJob";
import TauriJobFormShell from "../components/TauriJobFormShell";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
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
    <TauriJobFormShell
      title="Extract Messages"
      onBack={onBack}
      startLabel="Extract"
      runningLabel="Running…"
      running={running}
      log={log}
      startDisabled={!backupPath || !outputDir}
      onStart={() =>
        void start(
          () =>
            invokeExtract({
              source,
              path: backupPath,
              output_dir: outputDir,
            }),
          "Error starting extraction",
        )
      }
      onCancel={cancel}
    >
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
    </TauriJobFormShell>
  );
}
