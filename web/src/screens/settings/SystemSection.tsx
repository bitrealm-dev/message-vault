import { useState, useEffect, type CSSProperties } from "react";
import { isTauri } from "../../lib/tauri-check";
import { FFMPEG_TOOLS_STORAGE_KEY } from "../../lib/ffmpeg-tools";
import {
  getVaultWorkingDir,
  setVaultWorkingDir,
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

export function SystemSection() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [ffmpegStatus, setFfmpegStatus] = useState<Status>({ type: "idle" });
  const [ffmpegChecking, setFfmpegChecking] = useState(false);

  const [workingDir, setWorkingDir] = useState("");
  const [workingStatus, setWorkingStatus] = useState<Status>({ type: "idle" });

  const [rememberPaths, setRememberPaths] = useState(false);

  useEffect(() => {
    setWorkingDir(getVaultWorkingDir());
    setRememberPaths(getRememberImporterPaths());

    if (!isTauri()) return;

    const stored = localStorage.getItem(FFMPEG_TOOLS_STORAGE_KEY) || "";
    setFfmpegPath(stored);

    void (async () => {
      setFfmpegChecking(true);
      try {
        const result = stored.trim()
          ? await probeFfmpegTools(stored.trim())
          : await probeFfmpegTools(null);
        if (result.ok) {
          setFfmpegStatus({
            type: "success",
            message: stored.trim()
              ? formatFolderSuccess(result)
              : formatDefaultDiscovery(result),
          });
        } else if (result.error) {
          setFfmpegStatus({ type: "error", message: result.error });
        }
      } catch (err) {
        setFfmpegStatus({
          type: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        setFfmpegChecking(false);
      }
    })();
  }, []);

  const handleSaveFfmpeg = async () => {
    const path = ffmpegPath.trim();
    setFfmpegChecking(true);
    setFfmpegStatus({ type: "idle" });

    try {
      if (!path) {
        localStorage.removeItem(FFMPEG_TOOLS_STORAGE_KEY);
        const result = await setFfmpegToolsDir(null);
        if (result.ok) {
          setFfmpegStatus({ type: "success", message: formatDefaultDiscovery(result) });
        } else {
          setFfmpegStatus({
            type: "error",
            message: result.error ?? "ffmpeg and ffprobe not found on PATH",
          });
        }
        return;
      }

      const probe = await probeFfmpegTools(path);
      if (!probe.ok) {
        setFfmpegStatus({
          type: "error",
          message: probe.error ?? "ffmpeg and ffprobe not found in folder",
        });
        return;
      }

      const result = await setFfmpegToolsDir(path);
      localStorage.setItem(FFMPEG_TOOLS_STORAGE_KEY, path);
      setFfmpegStatus({ type: "success", message: formatFolderSuccess(result) });
    } catch (err) {
      setFfmpegStatus({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setFfmpegChecking(false);
    }
  };

  const handleSaveWorkingDir = () => {
    setVaultWorkingDir(workingDir);
    setWorkingStatus({
      type: "success",
      message: workingDir.trim()
        ? "Working directory saved."
        : "Working directory cleared. Import will write extract-output beside the backup path when no working directory is set.",
    });
  };

  const statusColor = (status: Status) =>
    status.type === "error"
      ? "var(--danger, #dc2626)"
      : status.type === "success"
        ? "var(--accent)"
        : "var(--muted)";

  if (!isTauri()) {
    return (
      <p style={{ margin: 0, fontSize: "0.875rem", color: "var(--muted)" }}>
        System settings (ffmpeg tools, working directory, and remembered importer paths) are
        available in the desktop app.
      </p>
    );
  }

  return (
    <div>
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
      <div style={{ marginTop: "1rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <Button
          onClick={() => void handleSaveFfmpeg()}
          disabled={ffmpegChecking}
          style={{ padding: "0.5rem 1.5rem" }}
        >
          {ffmpegChecking ? "Checking…" : "Save"}
        </Button>
        {ffmpegStatus.type !== "idle" && (
          <span style={{ fontSize: "0.875rem", color: statusColor(ffmpegStatus) }}>
            {ffmpegStatus.message}
          </span>
        )}
      </div>

      <div style={{ marginTop: "2rem" }}>
        <h3 style={sectionHeading}>Vault</h3>
        <FormRow label="Vault Working Directory">
          <PathPicker
            value={workingDir}
            onChange={setWorkingDir}
            directory
            placeholder="Optional — parent folder for import staging"
          />
        </FormRow>
        <p style={{ fontSize: "0.75rem", color: "var(--muted)", marginTop: "0.25rem" }}>
          Import creates a timestamped staging folder under this directory (for example
          staging-iphone-ios-260809-143022), matching the Slint GUI. Leave blank to write
          extract-output beside the selected backup path.
        </p>
        <div style={{ marginTop: "1rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
          <Button onClick={handleSaveWorkingDir} style={{ padding: "0.5rem 1.5rem" }}>
            Save
          </Button>
          {workingStatus.type !== "idle" && (
            <span style={{ fontSize: "0.875rem", color: statusColor(workingStatus) }}>
              {workingStatus.message}
            </span>
          )}
        </div>

        <label
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: "0.5rem",
            marginTop: "1.5rem",
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
      </div>
    </div>
  );
}
