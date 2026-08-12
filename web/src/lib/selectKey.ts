import type { Key } from "react";

/** Parse a React Aria selection key into one of the allowed string values. */
export function parseSelectKey<T extends string>(
  key: Key | null,
  allowed: readonly T[],
): T | null {
  if (key == null) return null;
  const s = String(key);
  return (allowed as readonly string[]).includes(s) ? (s as T) : null;
}
