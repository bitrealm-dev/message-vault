/** Backup import and export stay in the desktop app, and never on a guest session. */
export function canUseImportExport(isTauriApp: boolean, isGuest: boolean): boolean {
  return isTauriApp && !isGuest;
}

/**
 * Same rule as `canUseImportExport`, but a missing profile is not allowed.
 * Loading and a failed profile request both pass null here, so Import/Export stay hidden.
 */
export function canUseImportExportWithProfile(
  isTauriApp: boolean,
  profile: { is_guest?: boolean } | null | undefined,
): boolean {
  if (profile == null) {
    return false;
  }
  return canUseImportExport(isTauriApp, profile.is_guest === true);
}
