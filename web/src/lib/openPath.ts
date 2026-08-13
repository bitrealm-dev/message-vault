import { isTauri } from "./tauri-check";

/** Open a file or folder in the operating system's file manager. */
export async function openPathInExplorer(path: string): Promise<void> {
  const trimmed = path.trim();
  if (!trimmed) return;
  if (!isTauri()) {
    throw new Error("Opening folders requires the desktop app");
  }
  const { open } = await import("@tauri-apps/plugin-shell");
  await open(trimmed);
}
