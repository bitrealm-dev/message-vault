import type { Key } from "react";

/**
 * Convert a React Aria selection key into one of the allowed strings.
 * Returns null when the key is missing or not in the allowed list.
 */
export function parseSelectKey<T extends string>(key: Key | null, allowed: readonly T[]): T | null {
  if (key == null) return null;
  const s = String(key);
  for (const value of allowed) {
    if (value === s) return value;
  }
  return null;
}
