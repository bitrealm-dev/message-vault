export interface SavedGroup {
  id: string;
  name: string;
  query: string;
}

const STORAGE_KEY = "mv-saved-groups";

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
  localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
  return group;
}

export function removeGroup(id: string): void {
  const groups = listGroups().filter((g) => g.id !== id);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
}
