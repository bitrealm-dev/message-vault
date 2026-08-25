import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri-check";

/** Open a file or folder with the operating system's default handler. */
export async function openPathInExplorer(path: string): Promise<void> {
  const trimmed = path.trim();
  if (!trimmed) return;
  if (!isTauri()) {
    throw new Error("Opening folders requires the desktop app");
  }
  await invoke("open_path", { path: trimmed });
}
