/** Opaque keyset cursor for older-message pagination. */
export type MessagePageCursor = {
  timestamp: string;
  sortOrder: number;
  id: number;
};

function bytesToBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  const b64 =
    typeof btoa === "function"
      ? btoa(binary)
      : Buffer.from(bytes).toString("base64");
  return b64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function base64UrlToBytes(raw: string): Uint8Array {
  const b64 = raw.replace(/-/g, "+").replace(/_/g, "/");
  const padded = b64 + "=".repeat((4 - (b64.length % 4 || 4)) % 4);
  if (typeof atob === "function") {
    const binary = atob(padded);
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
    return out;
  }
  return new Uint8Array(Buffer.from(padded, "base64"));
}

export function encodeMessageCursor(cursor: MessagePageCursor): string {
  const json = JSON.stringify([cursor.timestamp, cursor.sortOrder, cursor.id]);
  return bytesToBase64Url(new TextEncoder().encode(json));
}

export function decodeMessageCursor(raw: string): MessagePageCursor | null {
  try {
    const json = new TextDecoder().decode(base64UrlToBytes(raw));
    const parsed = JSON.parse(json) as unknown;
    if (!Array.isArray(parsed) || parsed.length !== 3) return null;
    const [timestamp, sortOrder, id] = parsed;
    if (
      typeof timestamp !== "string" ||
      typeof sortOrder !== "number" ||
      !Number.isFinite(sortOrder) ||
      typeof id !== "number" ||
      !Number.isFinite(id)
    ) {
      return null;
    }
    return { timestamp, sortOrder, id };
  } catch {
    return null;
  }
}

function byTimestampThenId<T extends { id: number; timestamp: string }>(
  a: T,
  b: T,
): number {
  return a.timestamp < b.timestamp
    ? -1
    : a.timestamp > b.timestamp
      ? 1
      : a.id - b.id;
}

/** Merge message pages by id; keep chronological ascending order. */
export function mergeMessagePages<T extends { id: number; timestamp: string }>(
  existing: T[],
  incoming: T[],
): T[] {
  if (incoming.length === 0) {
    return existing.length <= 1
      ? existing
      : [...existing].sort(byTimestampThenId);
  }
  if (existing.length === 0) {
    return incoming.length <= 1
      ? incoming
      : [...incoming].sort(byTimestampThenId);
  }
  const byId = new Map<number, T>();
  for (const m of existing) byId.set(m.id, m);
  for (const m of incoming) byId.set(m.id, m);
  return [...byId.values()].sort(byTimestampThenId);
}

/** True when every message id is already present. */
export function messagesCoverIds(
  messages: { id: number }[],
  ids: number[],
): boolean {
  if (ids.length === 0) return true;
  const have = new Set(messages.map((m) => m.id));
  return ids.every((id) => have.has(id));
}
