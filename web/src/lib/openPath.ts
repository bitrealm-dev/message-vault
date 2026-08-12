import { isTauri } from "./tauri-check";

/** Open a file or directory in the OS file manager / default handler. */
export async function openPathInExplorer(path: string): Promise<void> {
  const trimmed = path.trim();
  if (!trimmed) return;
  if (!isTauri()) {
    throw new Error("Opening folders requires the desktop app");
  }
  const { open } = await import("@tauri-apps/plugin-shell");
  await open(trimmed);
}
