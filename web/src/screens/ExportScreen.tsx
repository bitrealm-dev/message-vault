import { useCallback, useState } from "react";
import { ListBoxItem } from "react-aria-components";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";
import Select, { selectItemClassName } from "../components/Select";
import TauriJobFormShell from "../components/TauriJobFormShell";
import { useTauriJob } from "../hooks/useTauriJob";
import { getBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { parseSelectKey } from "../lib/selectKey";
import { resolveExportStagingDir } from "../lib/system-settings";
import {
  EXPORT_FORMATS,
  type ExportFormat,
  invokeDeleteStaging,
  invokeFormat,
  invokePull,
} from "../lib/tauri";

const FORMAT_IDS = EXPORT_FORMATS.map((f) => f.id);

/** Label for the chosen format, for the success panel. */
function formatLabel(id: ExportFormat): string {
  return EXPORT_FORMATS.find((f) => f.id === id)?.label ?? id;
}

/**
 * Desktop export: `vault-pull` downloads the vault into a folder, and for any
 * format other than JSONL `message-reexport` rewrites that folder into the
 * chosen format.
 *
 * The two steps need two folders. `vault-pull` only writes JSONL, and
 * `message-reexport` refuses to convert a folder into itself, so a non-JSONL
 * export pulls into a staging folder first and converts out of it into the
 * folder the person picked. The staging folder is deleted either way, so a
 * failed conversion does not leave a copy of the vault behind.
 *
 * Shown only when Tauri is available (see LeftPanel).
 */
export default function ExportScreen() {
  const { token } = useAuth();
  const [savePath, setSavePath] = useState("");
  const [format, setFormat] = useState<ExportFormat>("jsonl");
  const [error, setError] = useState("");
  const [log, setLog] = useState<string[]>([]);
  // `running` only turns true once a job starts, which leaves two windows
  // where the Export button would be live mid-export: while the staging path
  // resolves (a `home_dir` round trip on the first export), and between the
  // pull and the conversion. A second job started in either window would
  // break `jobs.rs`'s "one job runs at a time" assumption, and two exports
  // begun in the same second would share a staging folder, so the first
  // cleanup would delete the second's files. This covers the whole run.
  const [busy, setBusy] = useState(false);
  const { running, finished, run, cancel } = useTauriJob();

  const appendLog = useCallback((line: string) => {
    setLog((prev) => [...prev, line]);
  }, []);

  const startExport = () => {
    if (busy) return;
    if (!token) {
      setError("Not authenticated");
      return;
    }
    setBusy(true);
    setError("");
    setLog([]);

    const pullInto = (outDir: string) =>
      run(
        () =>
          invokePull({
            base_url: getBaseUrl(),
            username: "",
            key: token,
            out_dir: outDir,
            query: "",
            skip_attachments: false,
          }),
        { onLog: appendLog },
      );

    void (async () => {
      try {
        if (format === "jsonl") {
          await pullInto(savePath);
          return;
        }
        const stagingDir = await resolveExportStagingDir();
        try {
          await pullInto(stagingDir);
          await run(
            () =>
              invokeFormat({
                input_dir: stagingDir,
                output_dir: savePath,
                output_format: format,
              }),
            { onLog: appendLog },
          );
        } finally {
          // Best effort: a staging folder left behind is worth a log line, not
          // a failed export the person cannot tell apart from a real one.
          try {
            await invokeDeleteStaging({ staging_dir: stagingDir });
          } catch (cleanupError: unknown) {
            appendLog(
              `Could not remove the staging folder ${stagingDir}: ${
                cleanupError instanceof Error ? cleanupError.message : String(cleanupError)
              }`,
            );
          }
        }
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        appendLog(`Error: ${message}`);
        setError(message);
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <TauriJobFormShell
      title="Export"
      requireTauri
      startLabel="Export"
      runningLabel="Exporting…"
      running={running || busy}
      log={log}
      startDisabled={!savePath || busy}
      onStart={startExport}
      onCancel={cancel}
      error={error}
      intro={
        <p className="mb-6 text-[0.875rem] text-muted">
          Export the entire vault (plus attachments) into a folder, in the format you choose.
        </p>
      }
      success={
        finished && !error ? (
          <div className="mt-4 rounded-md bg-ok-soft-bg p-4 text-[0.875rem]">
            Export complete. {formatLabel(format)} saved to {savePath}.
          </div>
        ) : null
      }
    >
      <FormRow label="Save to">
        <PathPicker
          value={savePath}
          onChange={setSavePath}
          directory
          placeholder="Choose folder…"
        />
      </FormRow>
      <FormRow label="Format">
        <Select
          selectedKey={format}
          onSelectionChange={(key) => {
            const next = parseSelectKey(key, FORMAT_IDS);
            if (next) setFormat(next);
          }}
          aria-label="Format"
          isDisabled={running || busy}
        >
          {EXPORT_FORMATS.map((option) => (
            <ListBoxItem key={option.id} id={option.id} className={selectItemClassName}>
              {option.label}
            </ListBoxItem>
          ))}
        </Select>
      </FormRow>
    </TauriJobFormShell>
  );
}
