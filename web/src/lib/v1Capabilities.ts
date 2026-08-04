/** V1: destructive GUI actions stay visible but disabled until V2. */
export const DELETION_UI_ENABLED = false;

export function isDeletionUiEnabled(): boolean {
  return DELETION_UI_ENABLED;
}

/** True when delete/trash/merge GUI actions must remain disabled (V1 policy). */
export function isDeletionUiBlocked(): boolean {
  return !DELETION_UI_ENABLED;
}
