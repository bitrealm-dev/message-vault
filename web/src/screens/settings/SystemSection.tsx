import { useEffect, useState } from "react";
import Button from "../../components/Button";
import FormRow from "../../components/FormRow";
import PathPicker from "../../components/PathPicker";
import { FFMPEG_TOOLS_STORAGE_KEY } from "../../lib/ffmpeg-tools";
import {
  getHomeDir,
  getRememberImporterPaths,
  getVaultWorkingDir,
  setRememberImporterPaths,
  setVaultWorkingDir,
} from "../../lib/system-settings";
import { type FfmpegToolsProbe, probeFfmpegTools, setFfmpegToolsDir } from "../../lib/tauri";
import { isTauri } from "../../lib/tauri-check";

type Status =
  | { type: "idle" }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

const sectionHeading = "m-0 mb-2 text-[12px] font-semibold uppercase tracking-[0.05em] text-muted";

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

export function SystemSection() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [workingDir, setWorkingDir] = useState("");
  const [homeDir, setHomeDir] = useState("");
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
      setHomeDir(home);
      const storedWorking = getVaultWorkingDir();
      setWorkingDir(storedWorking || home);

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
      const home = homeDir || (await getHomeDir());
      const workingTrimmed = workingDir.trim();
      // Empty or equal to home → restore default (no localStorage override).
      if (!workingTrimmed || (home && workingTrimmed === home)) {
        setVaultWorkingDir("");
        setWorkingDir(home);
      } else {
        setVaultWorkingDir(workingTrimmed);
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
        System settings (working directory, ffmpeg tools, and remembered importer paths) are
        available in the desktop app.
      </p>
    );
  }

  return (
    <div>
      <h3 className={sectionHeading}>Vault</h3>
      <FormRow label="Vault Working Directory">
        <PathPicker
          value={workingDir}
          onChange={setWorkingDir}
          directory
          placeholder={homeDir || "User home directory"}
        />
      </FormRow>
      <p className="mt-1 text-[0.75rem] text-muted">
        Import creates a timestamped staging folder under this directory (for example
        staging-iphone-ios-260809-143022), matching the Slint GUI. Defaults to your user home
        directory. Clear the field and save to restore that default.
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
