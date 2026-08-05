import { toPhoneE164 } from "./phoneE164";

/**
 * Handle identity types (mirrors `message_ir::HandleType`).
 * The type determines normalization and matching for a handle.
 */
export type HandleType = "phone" | "email" | "username" | "other";

/** iMessage-style: any handle containing `@` is treated as email. */
export function isEmailHandle(handle: string): boolean {
  return handle.includes("@");
}

/** Infer a handle type from the handle's shape when the source does not say. */
export function inferHandleType(raw: string): HandleType {
  const h = raw.trim();
  if (!h) return "other";
  if (h.includes("@")) return "email";
  const hasDigit = /\d/.test(h);
  const allPhoneChars = /^[\d+\- ().#*]+$/.test(h);
  if (hasDigit && allPhoneChars) return "phone";
  return "other";
}

/**
 * Canonical form of a handle for identity matching, per type.
 * Phone: E.164 when parseable (falls back to the trimmed raw). Email:
 * lowercased. Username/Other: verbatim (trimmed).
 */
export function normalizeHandle(raw: string, handleType: HandleType): string {
  const trimmed = raw.trim();
  switch (handleType) {
    case "phone": {
      const e164 = toPhoneE164(trimmed);
      return e164 ?? trimmed;
    }
    case "email":
      return trimmed.toLowerCase();
    case "username":
    case "other":
      return trimmed;
  }
}

/** Handles safe for contacts.csv `phones` column. */
export function phoneHandlesOnly(handles: string[]): string[] {
  return handles.filter((h) => h.trim() !== "" && !isEmailHandle(h));
}

/** Preferred display phone: first non-email handle, else first handle. */
export function preferredPhoneHandle(handles: string[]): string | null {
  const phones = phoneHandlesOnly(handles);
  if (phones[0]) return phones[0];
  const first = handles.map((h) => h.trim()).find(Boolean);
  return first ?? null;
}
