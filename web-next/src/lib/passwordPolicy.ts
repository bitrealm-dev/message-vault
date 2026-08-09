/** Passwords must be non-empty and strictly shorter than 100 characters. */
export const MAX_PASSWORD_LENGTH = 100;

export function validatePasswordPlaintext(password: string): string | null {
  if (!password) {
    return "password is required";
  }
  if (password.length >= MAX_PASSWORD_LENGTH) {
    return "password must be less than 100 characters";
  }
  return null;
}
