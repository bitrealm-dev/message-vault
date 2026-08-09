import { isTauri } from "./tauri-check";
import { setFfmpegToolsDir } from "./tauri";

export const FFMPEG_TOOLS_STORAGE_KEY = "mv-ffmpeg-path";

let initStarted = false;

/** Apply saved ffmpeg tools folder once at app startup (Tauri only). */
export function initFfmpegToolsFromStorage(): void {
  if (initStarted || !isTauri()) return;
  initStarted = true;

  const stored = localStorage.getItem(FFMPEG_TOOLS_STORAGE_KEY)?.trim();
  if (!stored) return;

  void setFfmpegToolsDir(stored).catch(() => {
    // Best-effort at startup; Settings shows detailed status on demand.
  });
}
