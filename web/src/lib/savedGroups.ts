export interface SavedGroup {
  id: string;
  name: string;
  query: string;
}

const STORAGE_KEY = "mv-saved-groups";

export const SAVED_GROUPS_CHANGED_EVENT = "mv-saved-groups-changed";

function notifySavedGroupsChanged(): void {
  try {
    globalThis.dispatchEvent?.(new Event(SAVED_GROUPS_CHANGED_EVENT));
  } catch {
    // Some browsers block custom events. Listing groups still works.
  }
}

/** Saved conversation groups stored in the browser. */
export function listGroups(): SavedGroup[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((g): g is SavedGroup => {
      if (typeof g !== "object" || g === null) return false;
      const row = g as Record<string, unknown>;
      return (
        typeof row.id === "string" && typeof row.name === "string" && typeof row.query === "string"
      );
    });
  } catch {
    return [];
  }
}

export function addGroup(name: string, query: string): SavedGroup {
  const groups = listGroups();
  const group: SavedGroup = { id: crypto.randomUUID(), name, query };
  groups.push(group);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
    notifySavedGroupsChanged();
  } catch {
    // Full or blocked storage should not break adding a group.
  }
  return group;
}

export function removeGroup(id: string): void {
  const groups = listGroups().filter((g) => g.id !== id);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
    notifySavedGroupsChanged();
  } catch {
    // Full or blocked storage should not break removing a group.
  }
}

/** Update a saved search's name and query. Id stays the same. */
export function updateGroup(id: string, name: string, query: string): SavedGroup | null {
  const groups = listGroups();
  const idx = groups.findIndex((g) => g.id === id);
  if (idx < 0) return null;
  const next: SavedGroup = { id, name, query };
  groups[idx] = next;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
    notifySavedGroupsChanged();
  } catch {
    // Full or blocked storage should not break renaming a group.
  }
  return next;
}

/** Unique name for an import group, adding " 2", " 3", … when the date is already used. */
export function uniqueImportGroupName(
  source: string,
  dateYmd: string,
  existingNames: string[],
): string {
  const base = `Import ${source} ${dateYmd}`;
  const taken = new Set(existingNames);
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base} ${n}`)) n += 1;
  return `${base} ${n}`;
}

/** True when this import added messages and should appear in Saved Groups. */
export function shouldSaveImportGroup(
  importSessionId: number | null | undefined,
  messagesInserted: number | null | undefined,
): boolean {
  return importSessionId != null && importSessionId > 0 && (messagesInserted ?? 0) > 0;
}

/** Local calendar date as YYYY-MM-DD. */
function localYmd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Save a group that opens this import's conversations, or skip when nothing was inserted. */
export function saveImportSavedGroup(args: {
  importSessionId: number;
  source: string;
  messagesInserted: number | null | undefined;
  now?: Date;
}): SavedGroup | null {
  if (!shouldSaveImportGroup(args.importSessionId, args.messagesInserted)) {
    return null;
  }
  const dateYmd = localYmd(args.now ?? new Date());
  const name = uniqueImportGroupName(
    args.source,
    dateYmd,
    listGroups().map((g) => g.name),
  );
  return addGroup(name, `import:${args.importSessionId}`);
}
