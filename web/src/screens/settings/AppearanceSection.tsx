import { useState, useEffect } from "react";
import { isTauri } from "../../lib/tauri-check";
import { FFMPEG_TOOLS_STORAGE_KEY } from "../../lib/ffmpeg-tools";
import {
  probeFfmpegTools,
  setFfmpegToolsDir,
  type FfmpegToolsProbe,
} from "../../lib/tauri";
import FormRow from "../../components/FormRow";
import ThemeSettings from "../../components/ThemeSettings";
import PathPicker from "../../components/PathPicker";
import Button from "../../components/Button";

type Status =
  | { type: "idle" }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

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

export function AppearanceSection() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [status, setStatus] = useState<Status>({ type: "idle" });
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;

    const stored = localStorage.getItem(FFMPEG_TOOLS_STORAGE_KEY) || "";
    setFfmpegPath(stored);

    void (async () => {
      setChecking(true);
      try {
        const result = stored.trim()
          ? await probeFfmpegTools(stored.trim())
          : await probeFfmpegTools(null);
        if (result.ok) {
          setStatus({
            type: "success",
            message: stored.trim()
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
      } finally {
        setChecking(false);
      }
    })();
  }, []);

  const handleSaveFfmpeg = async () => {
    const path = ffmpegPath.trim();
    setChecking(true);
    setStatus({ type: "idle" });

    try {
      if (!path) {
        localStorage.removeItem(FFMPEG_TOOLS_STORAGE_KEY);
        const result = await setFfmpegToolsDir(null);
        if (result.ok) {
          setStatus({ type: "success", message: formatDefaultDiscovery(result) });
        } else {
          setStatus({
            type: "error",
            message: result.error ?? "ffmpeg and ffprobe not found on PATH",
          });
        }
        return;
      }

      const probe = await probeFfmpegTools(path);
      if (!probe.ok) {
        setStatus({
          type: "error",
          message: probe.error ?? "ffmpeg and ffprobe not found in folder",
        });
        return;
      }

      const result = await setFfmpegToolsDir(path);
      localStorage.setItem(FFMPEG_TOOLS_STORAGE_KEY, path);
      setStatus({ type: "success", message: formatFolderSuccess(result) });
    } catch (err) {
      setStatus({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setChecking(false);
    }
  };

  const statusColor =
    status.type === "error"
      ? "var(--danger, #dc2626)"
      : status.type === "success"
        ? "var(--accent)"
        : "var(--muted)";

  return (
    <div>
      <ThemeSettings />

      {isTauri() && (
        <div style={{ marginTop: "2rem" }}>
          <h3
            style={{
              fontSize: "12px",
              fontWeight: 600,
              letterSpacing: "0.05em",
              textTransform: "uppercase",
              color: "var(--muted)",
              margin: "0 0 0.5rem",
            }}
          >
            Media
          </h3>
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
              disabled={checking}
              style={{ padding: "0.5rem 1.5rem" }}
            >
              {checking ? "Checking…" : "Save"}
            </Button>
            {status.type !== "idle" && (
              <span style={{ fontSize: "0.875rem", color: statusColor }}>{status.message}</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
