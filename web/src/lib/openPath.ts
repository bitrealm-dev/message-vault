import { invoke } from "@tauri-apps/api/core";
import { resolveImportStagingParent } from "./system-settings";
import { isTauri } from "./tauri-check";

/** Open a file or folder with the operating system's default handler. */
export async function openPathInExplorer(path: string): Promise<void> {
  const trimmed = path.trim();
  if (!trimmed) return;
  if (!isTauri()) {
    throw new Error("Opening folders requires the desktop app");
  }
  const stagingRoot = await resolveImportStagingParent();
  if (!stagingRoot) {
    throw new Error("Could not determine the import staging directory");
  }
  await invoke("open_path", { path: trimmed, stagingRoot });
}
