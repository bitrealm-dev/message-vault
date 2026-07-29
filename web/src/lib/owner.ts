import { loadAccount } from "./accounts";
import { currentAccountId } from "./accountScope";
import { isEmailHandle } from "./handleKind";
import { loadVaultOwner } from "./vaultOwner";

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

/** True when handle belongs to the vault owner or web account email. */
export function isOwnerHandle(handle: string): boolean {
  const trimmed = handle.trim();
  if (!trimmed) return false;

  const accountId = currentAccountId();
  const account = loadAccount(accountId);
  if (isEmailHandle(trimmed)) {
    return account.emails.some(
      (entry) => entry.email.toLowerCase() === trimmed.toLowerCase(),
    );
  }

  const digits = phoneDigits(trimmed);
  if (!digits) return false;
  const owner = loadVaultOwner(accountId);
  return owner.phones.some((p) => phoneDigits(p) === digits);
}

export function assertNotOwnerHandle(handle: string): void {
  if (isOwnerHandle(handle)) {
    throw new Error(
      "This number or email belongs to the vault owner or web account and cannot be assigned to a contact",
    );
  }
}
