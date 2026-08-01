import { hash, verify } from "@node-rs/argon2";

import {
  MAX_PASSWORD_LENGTH,
  validatePasswordPlaintext,
} from "./passwordPolicy";

export { MAX_PASSWORD_LENGTH, validatePasswordPlaintext };

export async function hashPassword(password: string): Promise<string> {
  const err = validatePasswordPlaintext(password);
  if (err) throw new Error(err);
  return hash(password);
}

export async function verifyPassword(
  passwordHash: string,
  password: string,
): Promise<boolean> {
  try {
    return await verify(passwordHash, password);
  } catch {
    return false;
  }
}

/**
 * Authenticate against a stored hash (or null = no password).
 * Empty password is accepted only when passwordHash is null/empty.
 */
export async function passwordsMatch(
  passwordHash: string | null | undefined,
  password: string,
): Promise<boolean> {
  const hasHash = Boolean(passwordHash);
  if (!hasHash) {
    return password === "";
  }
  return verifyPassword(passwordHash!, password);
}
