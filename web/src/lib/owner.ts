import { loadAccount } from "./accounts";
import { currentAccountId } from "./accountScope";
import { isEmailHandle } from "./handleKind";
import { loadAccountProfile } from "./accountProfile";

/** Strip non-digits; drop leading US country code 1 when 11 digits. */
export function phoneDigits(handle: string): string {
  let digits = handle.replace(/\D/g, "");
  if (digits.length === 11 && digits.startsWith("1")) {
    digits = digits.slice(1);
  }
  return digits;
}

export const VAULT_READ_ONLY_MESSAGE = "Vault is in read-only mode";

export function isVaultReadOnly(): boolean {
  return loadAccount(currentAccountId()).read_only;
}

/** Block browse/GUI vault mutations. Settings account APIs must not call this. */
export function assertVaultWritable(): void {
  if (isVaultReadOnly()) {
    throw new Error(VAULT_READ_ONLY_MESSAGE);
  }
}

/** True when an error message indicates the vault is locked for Web UI writes. */
export function isReadOnlyErrorMessage(message: string): boolean {
  return message.toLowerCase().includes("read-only");
}

/** Prefer 403 for read-only vault errors over a route's usual fallback status. */
export function mutationErrorStatus(
  message: string,
  fallback: number = 500,
): number {
  return isReadOnlyErrorMessage(message) ? 403 : fallback;
}

/**
 * Owner-handle predicate that loads the account and owner profile once.
 * Prefer this over calling {@link isOwnerHandle} in a loop: each account read
 * opens its own connection.
 */
export function ownerHandleMatcher(): (handle: string) => boolean {
  const accountId = currentAccountId();
  const emails = new Set(
    loadAccount(accountId).emails.map((entry) => entry.email.toLowerCase()),
  );
  const phones = new Set(
    loadAccountProfile(accountId).phones.map(phoneDigits).filter(Boolean),
  );

  return (handle: string) => {
    const trimmed = handle.trim();
    if (!trimmed) return false;
    if (isEmailHandle(trimmed)) return emails.has(trimmed.toLowerCase());
    const digits = phoneDigits(trimmed);
    return digits ? phones.has(digits) : false;
  };
}

/** True when handle belongs to this account's phones or emails. */
export function isOwnerHandle(handle: string): boolean {
  return ownerHandleMatcher()(handle);
}

export function assertNotOwnerHandle(handle: string): void {
  if (isOwnerHandle(handle)) {
    throw new Error(
      "This number or email belongs to your account and cannot be assigned to a contact",
    );
  }
}
