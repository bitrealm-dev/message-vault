/** Backup import and export stay in the desktop app, and never on a guest session. */
export function canUseImportExport(isTauriApp: boolean, isGuest: boolean): boolean {
  return isTauriApp && !isGuest;
}
