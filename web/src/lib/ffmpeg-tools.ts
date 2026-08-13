import { isTauri } from "./tauri-check";
import { setFfmpegToolsDir } from "./tauri";

export const FFMPEG_TOOLS_STORAGE_KEY = "mv-ffmpeg-path";

let initStarted = false;

/** Apply the saved ffmpeg tools folder once when the desktop app starts. */
export function initFfmpegToolsFromStorage(): void {
  if (initStarted || !isTauri()) return;
  initStarted = true;

  const stored = localStorage.getItem(FFMPEG_TOOLS_STORAGE_KEY)?.trim();
  if (!stored) return;

  void setFfmpegToolsDir(stored).catch(() => {
    // Startup is best-effort. Settings shows a detailed status when the user opens it.
  });
}
