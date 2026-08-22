export type ContactNameSort = "first" | "last";
export type ContactSortOrder = "asc" | "desc";

export interface ContactNameSortState {
  sort: ContactNameSort;
  order: ContactSortOrder;
}

export const DEFAULT_CONTACT_NAME_SORT = {
  sort: "last",
  order: "asc",
} as const satisfies ContactNameSortState;

const STORAGE_KEY = "contactNameSort:v1";

/** First word and last word of a display name. A single word is used for both. */
export function splitContactName(name: string): { first: string; last: string } {
  const trimmed = name.trim();
  if (!trimmed) return { first: "", last: "" };

  if (trimmed.includes(",")) {
    const [lastPart, firstPart] = trimmed.split(",").map((s) => s.trim());
    const last = lastPart || firstPart || "";
    const first = firstPart || lastPart || "";
    return { first, last };
  }

  const parts = trimmed.split(/\s+/).filter(Boolean);
  const only = parts[0] ?? "";
  if (parts.length <= 1) return { first: only, last: only };
  return { first: only, last: parts[parts.length - 1] ?? only };
}

/** A–Z from the active name field, or `#` for numbers and symbols. */
export function contactSortLetter(name: string, sort: ContactNameSort): string {
  const parts = splitContactName(name);
  const src = sort === "first" ? parts.first : parts.last;
  const ch = src.charAt(0).toUpperCase();
  return ch >= "A" && ch <= "Z" ? ch : "#";
}

/** Split an already-sorted list into letter groups. Order of groups is kept. */
export function groupByLetter<T>(
  items: readonly T[],
  letterOf: (item: T) => string,
): ReadonlyArray<readonly [string, readonly T[]]> {
  const groups: Array<[string, T[]]> = [];
  for (const item of items) {
    const letter = letterOf(item);
    const last = groups[groups.length - 1];
    if (last && last[0] === letter) {
      last[1].push(item);
    } else {
      groups.push([letter, [item]]);
    }
  }
  return groups;
}

export function compareContactsByName(
  a: string,
  b: string,
  sort: ContactNameSort,
  order: ContactSortOrder,
): number {
  const pa = splitContactName(a);
  const pb = splitContactName(b);
  const primary = sort === "first" ? "first" : "last";
  const secondary = sort === "first" ? "last" : "first";
  let cmp = pa[primary].localeCompare(pb[primary], undefined, {
    sensitivity: "base",
  });
  if (cmp === 0) {
    cmp = pa[secondary].localeCompare(pb[secondary], undefined, {
      sensitivity: "base",
    });
  }
  return order === "desc" ? -cmp : cmp;
}

function isNameSort(value: unknown): value is ContactNameSort {
  return value === "first" || value === "last";
}

function isSortOrder(value: unknown): value is ContactSortOrder {
  return value === "asc" || value === "desc";
}

export function loadContactNameSort(): ContactNameSortState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_CONTACT_NAME_SORT };
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) {
      return { ...DEFAULT_CONTACT_NAME_SORT };
    }
    const rec = parsed as Record<string, unknown>;
    return {
      sort: isNameSort(rec.sort) ? rec.sort : DEFAULT_CONTACT_NAME_SORT.sort,
      order: isSortOrder(rec.order) ? rec.order : DEFAULT_CONTACT_NAME_SORT.order,
    };
  } catch {
    return { ...DEFAULT_CONTACT_NAME_SORT };
  }
}

export function saveContactNameSort(state: ContactNameSortState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // private browsing / quota
  }
}
