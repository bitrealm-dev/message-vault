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

/** Digits only for comparison (`+1 555-123-4567` → `15551234567`). */
export function normalizePhoneDigits(phone: string): string {
  return phone.replace(/\D/g, "");
}

/**
 * True when at least one owner phone digit-matches a profile phone.
 * Empty owner list or empty profile → false (nothing can match).
 */
export function ownerPhonesMatchProfile(
  ownerPhones: readonly string[],
  profilePhones: readonly string[],
): boolean {
  if (ownerPhones.length === 0 || profilePhones.length === 0) return false;
  const profileDigits = new Set(
    profilePhones.map(normalizePhoneDigits).filter((d) => d.length > 0),
  );
  if (profileDigits.size === 0) return false;
  return ownerPhones.some((phone) => {
    const digits = normalizePhoneDigits(phone);
    return digits.length > 0 && profileDigits.has(digits);
  });
}

/**
 * Whether SBR Import should require the mismatch acknowledgment checkbox.
 * Empty profile always needs ack once the profile has loaded successfully.
 * Fetch failure → false (fail open; caller still waits for ready).
 */
export function ownerPhonesNeedMismatchAck(
  ownerPhones: readonly string[],
  profilePhones: readonly string[],
  opts: { ready: boolean; fetchFailed: boolean },
): boolean {
  if (!opts.ready || opts.fetchFailed) return false;
  if (profilePhones.length === 0) return true;
  if (ownerPhones.length === 0) return false;
  return !ownerPhonesMatchProfile(ownerPhones, profilePhones);
}
