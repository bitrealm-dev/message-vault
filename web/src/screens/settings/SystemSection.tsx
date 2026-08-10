import { useState, useEffect, type CSSProperties } from "react";
import { isTauri } from "../../lib/tauri-check";
import { FFMPEG_TOOLS_STORAGE_KEY } from "../../lib/ffmpeg-tools";
import {
  getVaultWorkingDir,
  setVaultWorkingDir,
  getHomeDir,
  getRememberImporterPaths,
  setRememberImporterPaths,
} from "../../lib/system-settings";
import {
  probeFfmpegTools,
  setFfmpegToolsDir,
  type FfmpegToolsProbe,
} from "../../lib/tauri";
import FormRow from "../../components/FormRow";
import PathPicker from "../../components/PathPicker";
import Button from "../../components/Button";

type Status =
  | { type: "idle" }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

const sectionHeading: CSSProperties = {
  fontSize: "12px",
  fontWeight: 600,
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  color: "var(--muted)",
  margin: "0 0 0.5rem",
};

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
      <p style={{ margin: 0, fontSize: "0.875rem", color: "var(--muted)" }}>
        System settings (working directory, ffmpeg tools, and remembered importer paths) are
        available in the desktop app.
      </p>
    );
  }

  return (
    <div>
      <h3 style={sectionHeading}>Vault</h3>
      <FormRow label="Vault Working Directory">
        <PathPicker
          value={workingDir}
          onChange={setWorkingDir}
          directory
          placeholder={homeDir || "User home directory"}
        />
      </FormRow>
      <p style={{ fontSize: "0.75rem", color: "var(--muted)", marginTop: "0.25rem" }}>
        Import creates a timestamped staging folder under this directory (for example
        staging-iphone-ios-260809-143022), matching the Slint GUI. Defaults to your user home
        directory. Clear the field and save to restore that default.
      </p>

      <label
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: "0.5rem",
          marginTop: "1.25rem",
          fontSize: "0.875rem",
          cursor: "pointer",
        }}
      >
        <input
          type="checkbox"
          checked={rememberPaths}
          onChange={(e) => {
            const on = e.target.checked;
            setRememberPaths(on);
            setRememberImporterPaths(on);
          }}
          style={{ marginTop: "0.15rem" }}
        />
        <span>
          Remember importer paths
          <span
            style={{
              display: "block",
              fontSize: "0.75rem",
              color: "var(--muted)",
              marginTop: "0.25rem",
            }}
          >
            When enabled, Import restores the last backup path for each import source.
          </span>
        </span>
      </label>

      <div style={{ marginTop: "2rem" }}>
        <h3 style={sectionHeading}>Media</h3>
        <FormRow label="ffmpeg tools folder">
          <PathPicker
            value={ffmpegPath}
            onChange={setFfmpegPath}
            directory
            placeholder="Uses system PATH by default"
          />
        </FormRow>
        <p style={{ fontSize: "0.75rem", color: "var(--muted)", marginTop: "0.25rem" }}>
          Folder must contain both ffmpeg and ffprobe. Leave blank to use system PATH.{" "}
          <a
            href="https://bitrealm-dev.github.io/message-vault-io/ffmpeg"
            target="_blank"
            rel="noopener"
            style={{ color: "var(--accent)" }}
          >
            Install help
          </a>
        </p>
      </div>

      <div style={{ marginTop: "1.5rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <Button
          onClick={() => void handleSave()}
          disabled={saving}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          {saving ? "Saving…" : "Save"}
        </Button>
        {status.type !== "idle" && (
          <span style={{ fontSize: "0.875rem", color: statusColor(status) }}>{status.message}</span>
        )}
      </div>
    </div>
  );
}
