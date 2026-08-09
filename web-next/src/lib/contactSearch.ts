import type { ContactListItem } from "./types";

function normalizeName(value: string | null | undefined): string {
  return (value ?? "").trim().toLowerCase();
}

function phoneDigits(value: string | null | undefined): string {
  return (value ?? "").replace(/\D/g, "");
}

/**
 * Filter contacts by a contiguous character/digit sequence in first name,
 * last name (either order), or phone — independent of list sort order.
 */
export function searchContacts(
  contacts: ContactListItem[],
  query: string,
): ContactListItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return contacts;

  const qDigits = q.replace(/\D/g, "");

  return contacts.filter((c) => {
    const preferred = normalizeName(c.preferredName);
    const first = normalizeName(c.firstName);
    const last = normalizeName(c.lastName);
    const display = normalizeName(c.displayName);
    const nameFields = [
      preferred,
      first,
      last,
      display,
      `${first} ${last}`.trim(),
      `${last} ${first}`.trim(),
    ];

    if (nameFields.some((field) => field.length > 0 && field.includes(q))) {
      return true;
    }

    const handle = (c.preferredHandle ?? "").toLowerCase();
    if (handle.includes(q)) return true;

    if (qDigits.length > 0) {
      const digits = phoneDigits(c.preferredHandle);
      if (digits.includes(qDigits)) return true;
    }

    return false;
  });
}
