import { useCallback, useEffect, useId, useRef, useState } from "react";
import { CheckIcon, XIcon } from "../../components/icons";
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

const sectionHeading = "m-0 mb-2 text-[12px] font-semibold uppercase tracking-[0.05em] text-muted";

const EXAMPLE_STAGING = "staging-iphone-ios-260809-143022";

/** Shared label + control grid so Vault and Media path fields share one nowrap label column. */
const settingsGrid = "grid grid-cols-[13.5rem_minmax(0,1fr)] items-center gap-x-3 gap-y-1";
const settingsLabel = "whitespace-nowrap text-[0.875rem] font-medium text-text";
const settingsHelp = "col-start-2 pl-2 text-[0.75rem] text-muted";

const FFMPEG_DEBOUNCE_MS = 300;

/** Example path shown under the Import staging directory field. */
function stagingHelpExample(stagingDir: string, defaultDir: string): string {
  const trimmed = stagingDir.trim().replace(/[/\\]+$/, "");
  const defaultTrimmed = defaultDir.trim().replace(/[/\\]+$/, "");
  if (!trimmed || (defaultTrimmed && trimmed === defaultTrimmed)) {
    return `~/message-vault/${EXAMPLE_STAGING}`;
  }
  return `${trimmed}/${EXAMPLE_STAGING}`;
}

function persistFfmpegDir(dir: string): void {
  const trimmed = dir.trim();
  if (!trimmed) {
    localStorage.removeItem(FFMPEG_TOOLS_STORAGE_KEY);
    return;
  }
  localStorage.setItem(FFMPEG_TOOLS_STORAGE_KEY, trimmed);
}

function ToolStatusRow({ name, path }: { name: "ffmpeg" | "ffprobe"; path: string | null }) {
  if (path) {
    const label = `Found ${name} - ${path}`;
    return (
      <li className="flex items-start gap-1.5 text-[0.75rem] text-text" aria-label={label}>
        <CheckIcon size={14} className="mt-0.5 shrink-0 text-ok" />
        <span>
          Found <code className="font-mono text-[0.7rem]">{name}</code>
          {" - "}
          <code className="font-mono text-[0.7rem]">{path}</code>
        </span>
      </li>
    );
  }
  const label = `${name} not found`;
  return (
    <li className="flex items-start gap-1.5 text-[0.75rem] text-text" aria-label={label}>
      <XIcon size={14} className="mt-0.5 shrink-0 text-danger" />
      <span>
        <code className="font-mono text-[0.7rem]">{name}</code> not found
      </span>
    </li>
  );
}

export function SystemSection() {
  const stagingId = useId();
  const ffmpegId = useId();
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [stagingDir, setStagingDir] = useState("");
  const [defaultStagingDir, setDefaultStagingDir] = useState("");
  const [rememberPaths, setRememberPaths] = useState(false);
  const [probe, setProbe] = useState<FfmpegToolsProbe | null>(null);
  const ffmpegDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const ffmpegApplyGen = useRef(0);

  const runFfmpegApply = useCallback(async (dir: string) => {
    const gen = ++ffmpegApplyGen.current;
    const trimmed = dir.trim();
    try {
      if (!trimmed) {
        const result = await setFfmpegToolsDir(null);
        if (gen !== ffmpegApplyGen.current) return;
        persistFfmpegDir("");
        setProbe(result);
        return;
      }

      const probed = await probeFfmpegTools(trimmed);
      if (gen !== ffmpegApplyGen.current) return;
      setProbe(probed);
      if (!probed.ok) return;

      const applied = await setFfmpegToolsDir(trimmed);
      if (gen !== ffmpegApplyGen.current) return;
      setProbe(applied);
      if (applied.ok) persistFfmpegDir(trimmed);
    } catch {
      if (gen !== ffmpegApplyGen.current) return;
      setProbe({
        ok: false,
        ffmpeg_path: null,
        ffprobe_path: null,
        error: "Could not check ffmpeg tools",
      });
    }
  }, []);

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
      await runFfmpegApply(storedFfmpeg);
    })();

    return () => {
      if (ffmpegDebounceRef.current) clearTimeout(ffmpegDebounceRef.current);
    };
  }, [runFfmpegApply]);

  const onStagingPathChange = (next: string) => {
    setStagingDir(next);
    const defaultDir = defaultStagingDir;
    const trimmed = next.trim();
    // Empty or equal to the default → no override (import uses the default parent).
    if (!trimmed || (defaultDir && trimmed === defaultDir)) {
      setImportStagingDir("");
      return;
    }
    // Relative / filesystem root while typing: keep the field, do not persist yet.
    if (!isUsableImportStagingParent(trimmed)) {
      return;
    }
    setImportStagingDir(trimmed);
  };

  const onFfmpegPathChange = (next: string) => {
    setFfmpegPath(next);
    if (ffmpegDebounceRef.current) clearTimeout(ffmpegDebounceRef.current);
    ffmpegDebounceRef.current = setTimeout(() => {
      void runFfmpegApply(next);
    }, FFMPEG_DEBOUNCE_MS);
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
      <div className={settingsGrid}>
        <label htmlFor={stagingId} className={settingsLabel}>
          Import staging directory
        </label>
        <div>
          <PathPicker
            id={stagingId}
            value={stagingDir}
            onChange={onStagingPathChange}
            directory
            placeholder={defaultStagingDir || "~/message-vault"}
          />
        </div>
        <p className={settingsHelp}>
          Temporary import files are written here. For example {helpExample}
        </p>
      </div>

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
        <div className={settingsGrid}>
          <label htmlFor={ffmpegId} className={settingsLabel}>
            ffmpeg directory
          </label>
          <div>
            <PathPicker
              id={ffmpegId}
              value={ffmpegPath}
              onChange={onFfmpegPathChange}
              directory
              placeholder="Uses system PATH by default"
            />
          </div>
          <p className={settingsHelp}>
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
          {probe ? (
            <ul className={`${settingsHelp} mt-1 list-none space-y-1 p-0`}>
              <ToolStatusRow name="ffmpeg" path={probe.ffmpeg_path} />
              <ToolStatusRow name="ffprobe" path={probe.ffprobe_path} />
            </ul>
          ) : null}
        </div>
      </div>
    </div>
  );
}
