/** Split raw input into phone tokens (comma-separated; spaces inside a number are kept). */
export function splitPhoneTokenInput(raw: string): string[] {
  return raw
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

/** Append new phones to the list, skipping empties and duplicates. */
export function commitPhoneTokens(current: readonly string[], raw: string): string[] {
  const next = [...current];
  const seen = new Set(current.map((p) => p.trim()));
  for (const phone of splitPhoneTokenInput(raw)) {
    if (seen.has(phone)) continue;
    seen.add(phone);
    next.push(phone);
  }
  return next;
}

/** Remove one phone by value (first match). */
export function removePhoneToken(current: readonly string[], phone: string): string[] {
  const index = current.indexOf(phone);
  if (index < 0) return [...current];
  return [...current.slice(0, index), ...current.slice(index + 1)];
}
