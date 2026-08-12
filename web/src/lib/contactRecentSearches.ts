const CONTACT_RECENT_SEARCHES_KEY = "mv-contact-recent-searches:v1";
const CONTACT_RECENT_SEARCHES_MAX = 10;

function readRaw(): unknown {
  try {
    const raw = localStorage.getItem(CONTACT_RECENT_SEARCHES_KEY);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function loadContactRecentSearches(): string[] {
  const parsed = readRaw();
  if (!Array.isArray(parsed)) return [];
  return parsed
    .filter((x): x is string => typeof x === "string")
    .map((s) => s.trim())
    .filter(Boolean)
    .slice(0, CONTACT_RECENT_SEARCHES_MAX);
}

function saveContactRecentSearches(queries: string[]): void {
  try {
    localStorage.setItem(
      CONTACT_RECENT_SEARCHES_KEY,
      JSON.stringify(queries.slice(0, CONTACT_RECENT_SEARCHES_MAX)),
    );
  } catch {
    /* private mode / quota */
  }
}

export function clearContactRecentSearches(): void {
  try {
    localStorage.removeItem(CONTACT_RECENT_SEARCHES_KEY);
  } catch {
    /* ignore */
  }
}

export function pushContactRecentSearch(query: string): string[] {
  const q = query.trim();
  if (!q) return loadContactRecentSearches();
  const next = [q, ...loadContactRecentSearches().filter((x) => x !== q)].slice(
    0,
    CONTACT_RECENT_SEARCHES_MAX,
  );
  saveContactRecentSearches(next);
  return next;
}
