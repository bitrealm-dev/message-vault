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
    // Ignore unavailable or restricted browser event APIs.
  }
}

export function listGroups(): SavedGroup[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
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
    // Keep storage failures from interrupting the caller.
  }
  return group;
}

export function removeGroup(id: string): void {
  const groups = listGroups().filter((g) => g.id !== id);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
    notifySavedGroupsChanged();
  } catch {
    // Keep storage failures from interrupting the caller.
  }
}

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

export function shouldSaveImportGroup(
  importSessionId: number | null | undefined,
  messagesInserted: number | null | undefined,
): boolean {
  return (
    importSessionId != null &&
    importSessionId > 0 &&
    (messagesInserted ?? 0) > 0
  );
}

function localYmd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

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
