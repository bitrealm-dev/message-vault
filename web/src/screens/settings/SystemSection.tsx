import { useEffect, useState } from "react";
import Button from "../../components/Button";
import FormRow from "../../components/FormRow";
import PathPicker from "../../components/PathPicker";
import { FFMPEG_TOOLS_STORAGE_KEY } from "../../lib/ffmpeg-tools";
import {
  defaultImportStagingDir,
  getHomeDir,
  getImportStagingDir,
  getRememberImporterPaths,
  isUsableImportStagingParent,
  setImportStagingDir,
  setRememberImporterPaths,
} from "../../lib/system-settings";
import { type FfmpegToolsProbe, probeFfmpegTools, setFfmpegToolsDir } from "../../lib/tauri";
import { isTauri } from "../../lib/tauri-check";

type Status =
  | { type: "idle" }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

const sectionHeading = "m-0 mb-2 text-[12px] font-semibold uppercase tracking-[0.05em] text-muted";

const EXAMPLE_STAGING = "staging-iphone-ios-260809-143022";

function formatProbePaths(probe: FfmpegToolsProbe): string {
  const parts: string[] = [];
  if (probe.ffmpeg_path) parts.push(`ffmpeg: ${probe.ffmpeg_path}`);
  if (probe.ffprobe_path) parts.push(`ffprobe: ${probe.ffprobe_path}`);
  return parts.join(" · ");
}

function formatDefaultDiscovery(probe: FfmpegToolsProbe): string {
  const paths = formatProbePaths(probe);
  return paths ? `Using default discovery. ${paths}` : "Using default discovery.";
}

function formatFolderSuccess(probe: FfmpegToolsProbe): string {
  const paths = formatProbePaths(probe);
  return paths || "ffmpeg tools folder saved.";
}

function statusColor(status: Status): string {
  if (status.type === "error") return "var(--danger, #dc2626)";
  if (status.type === "success") return "var(--accent)";
  return "var(--muted)";
}

/** Example path shown under the Import Staging Directory field. */
function stagingHelpExample(stagingDir: string, defaultDir: string): string {
  const trimmed = stagingDir.trim().replace(/[/\\]+$/, "");
  const defaultTrimmed = defaultDir.trim().replace(/[/\\]+$/, "");
  if (!trimmed || (defaultTrimmed && trimmed === defaultTrimmed)) {
    return `~/message-vault/${EXAMPLE_STAGING}`;
  }
  return `${trimmed}/${EXAMPLE_STAGING}`;
}

export function SystemSection() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [stagingDir, setStagingDir] = useState("");
  const [defaultStagingDir, setDefaultStagingDir] = useState("");
  const [rememberPaths, setRememberPaths] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<Status>({ type: "idle" });

  useEffect(() => {
    if (!isTauri()) return;

    setRememberPaths(getRememberImporterPaths());
    const storedFfmpeg = localStorage.getItem(FFMPEG_TOOLS_STORAGE_KEY) || "";
    setFfmpegPath(storedFfmpeg);

    void (async () => {
      const home = await getHomeDir();
      const defaultDir = defaultImportStagingDir(home);
      setDefaultStagingDir(defaultDir);
      const storedStaging = getImportStagingDir();
      setStagingDir(storedStaging || defaultDir);

      try {
        const result = storedFfmpeg.trim()
          ? await probeFfmpegTools(storedFfmpeg.trim())
          : await probeFfmpegTools(null);
        if (result.ok) {
          setStatus({
            type: "success",
            message: storedFfmpeg.trim()
              ? formatFolderSuccess(result)
              : formatDefaultDiscovery(result),
          });
        } else if (result.error) {
          setStatus({ type: "error", message: result.error });
        }
      } catch (err) {
        setStatus({
          type: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    })();
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setStatus({ type: "idle" });

    try {
      const home = (await getHomeDir()) || "";
      const defaultDir = defaultImportStagingDir(home) || defaultStagingDir;
      setDefaultStagingDir(defaultDir);
      const stagingTrimmed = stagingDir.trim();
      // Empty, equal to the default, or unusable (relative / filesystem root)
      // → restore default (no localStorage override).
      if (
        !stagingTrimmed ||
        (defaultDir && stagingTrimmed === defaultDir) ||
        !isUsableImportStagingParent(stagingTrimmed)
      ) {
        setImportStagingDir("");
        setStagingDir(defaultDir);
      } else {
        setImportStagingDir(stagingTrimmed);
      }

      const ffmpegTrimmed = ffmpegPath.trim();
      if (!ffmpegTrimmed) {
        localStorage.removeItem(FFMPEG_TOOLS_STORAGE_KEY);
        const result = await setFfmpegToolsDir(null);
        if (!result.ok) {
          setStatus({
            type: "error",
            message: result.error ?? "ffmpeg and ffprobe not found on PATH",
          });
          return;
        }
        setStatus({
          type: "success",
          message: `Settings saved. ${formatDefaultDiscovery(result)}`,
        });
        return;
      }

      const probe = await probeFfmpegTools(ffmpegTrimmed);
      if (!probe.ok) {
        setStatus({
          type: "error",
          message: probe.error ?? "ffmpeg and ffprobe not found in folder",
        });
        return;
      }

      const result = await setFfmpegToolsDir(ffmpegTrimmed);
      localStorage.setItem(FFMPEG_TOOLS_STORAGE_KEY, ffmpegTrimmed);
      setStatus({
        type: "success",
        message: `Settings saved. ${formatFolderSuccess(result)}`,
      });
    } catch (err) {
      setStatus({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSaving(false);
    }
  };

  if (!isTauri()) {
    return (
      <p className="m-0 text-[0.875rem] text-muted">
        System settings (import staging directory, ffmpeg tools, and remembered importer paths) are
        available in the desktop app.
      </p>
    );
  }

  const helpExample = stagingHelpExample(stagingDir, defaultStagingDir);

  return (
    <div>
      <h3 className={sectionHeading}>Vault</h3>
      <FormRow label="Import Staging Directory">
        <PathPicker
          value={stagingDir}
          onChange={setStagingDir}
          directory
          placeholder={defaultStagingDir || "~/message-vault"}
        />
      </FormRow>
      <p className="mt-1 text-[0.75rem] text-muted">
        Temporary import files are written here. For example {helpExample}
      </p>

      <label className="mt-5 flex cursor-pointer items-start gap-2 text-[0.875rem]">
        <input
          type="checkbox"
          checked={rememberPaths}
          onChange={(e) => {
            const on = e.target.checked;
            setRememberPaths(on);
            setRememberImporterPaths(on);
          }}
          className="mt-[0.15rem]"
        />
        <span>
          Remember importer paths
          <span className="mt-1 block text-[0.75rem] text-muted">
            When enabled, Import restores the last backup path for each import source.
          </span>
        </span>
      </label>

      <div className="mt-8">
        <h3 className={sectionHeading}>Media</h3>
        <FormRow label="ffmpeg tools folder">
          <PathPicker
            value={ffmpegPath}
            onChange={setFfmpegPath}
            directory
            placeholder="Uses system PATH by default"
          />
        </FormRow>
        <p className="mt-1 text-[0.75rem] text-muted">
          Folder must contain both ffmpeg and ffprobe. Leave blank to use system PATH.{" "}
          <a
            href="https://bitrealm.io/vault/user/how-to/media-and-privacy/"
            target="_blank"
            rel="noopener"
            className="text-accent"
          >
            Install help
          </a>
        </p>
      </div>

      <div className="mt-6 flex items-center gap-3">
        <Button onClick={() => void handleSave()} disabled={saving} className="!px-6 !py-2">
          {saving ? "Saving…" : "Save"}
        </Button>
        {status.type !== "idle" && (
          <span className="text-[0.875rem]" style={{ color: statusColor(status) }}>
            {status.message}
          </span>
        )}
      </div>
    </div>
  );
}
