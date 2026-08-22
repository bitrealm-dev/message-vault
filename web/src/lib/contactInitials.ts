/** Initials and a stable avatar color for contact list chips. */

const AVATAR_COLORS = [
  "#c45c6a",
  "#7c6bc4",
  "#3d8b7a",
  "#b87a3d",
  "#4a7eb8",
  "#9a5fa0",
  "#5a8f4a",
  "#b85c8a",
] as const;

/** First letter of a name, or empty when there is no usable character. */
function firstLetter(raw: string | null | undefined): string {
  if (!raw) return "";
  const t = raw.trim();
  if (!t) return "";
  const ch = t.charAt(0).toUpperCase();
  return /[A-Z0-9]/.test(ch) ? ch : "";
}

/** Two-letter initials from a contact's name fields. */
export function contactInitials(c: {
  preferredName?: string | null;
  firstName?: string | null;
  lastName?: string | null;
  sortFirst?: string;
  sortLast?: string;
  displayName?: string;
}): string {
  const preferred = (c.preferredName ?? c.displayName ?? "").trim();
  const first = firstLetter(c.firstName) || firstLetter(c.sortFirst) || firstLetter(preferred);
  const last = firstLetter(c.lastName) || firstLetter(c.sortLast) || "";

  if (first && last && first !== last) return `${first}${last}`;
  if (first && last) return first;

  // Fall back to two letters from the preferred or display name.
  const name = preferred;
  if (name.includes(",")) {
    const [ln, fn] = name.split(",").map((s) => s.trim());
    const a = firstLetter(fn) || firstLetter(ln);
    const b = firstLetter(ln);
    if (a && b && a !== b) return `${a}${b}`;
    return a || b || "?";
  }
  const parts = name.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    const a = firstLetter(parts[0]);
    const b = firstLetter(parts[parts.length - 1]);
    if (a && b) return `${a}${b}`;
  }
  if (first) return first;
  const single = firstLetter(name);
  return single || "?";
}

/** Number used to pick a color from the palette. Same string always gives the same number. */
function hashString(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    n = (n * 31 + s.charCodeAt(i)) >>> 0;
  }
  return n;
}

/** Strip a phone or email so the same person keeps the same avatar color. */
function normalizeHandle(handle: string | null | undefined): string {
  const t = (handle ?? "").trim().toLowerCase();
  if (!t) return "";
  const digits = t.replace(/\D/g, "");
  // Use digits for phone-like handles. Keep the full string for emails.
  if (digits.length >= 7) return digits;
  return t;
}

function normalizeName(name: string | null | undefined): string {
  return (name ?? "").trim().toLowerCase().replace(/\s+/g, " ");
}

/**
 * Pick a palette color from the display name and preferred handle.
 * The same person keeps the same color even if their contact id changes.
 */
export function contactAvatarColor(input: {
  displayName?: string | null;
  preferredName?: string | null;
  preferredHandle?: string | null;
  firstName?: string | null;
  lastName?: string | null;
}): string {
  const name =
    normalizeName(input.preferredName) ||
    normalizeName(input.displayName) ||
    normalizeName([input.firstName, input.lastName].filter(Boolean).join(" "));
  const handle = normalizeHandle(input.preferredHandle);
  const seed = `${name}\0${handle}`;
  return AVATAR_COLORS[hashString(seed) % AVATAR_COLORS.length]!;
}
