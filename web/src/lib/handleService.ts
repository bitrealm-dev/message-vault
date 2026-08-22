/** Messaging service ids used on profiles, setup, and contacts. */
export type HandleService = "phone" | "email" | "whatsapp";

export const HANDLE_SERVICES = [
  "phone",
  "email",
  "whatsapp",
] as const satisfies readonly HandleService[];

/** Contact drawer identity picker: phone and WhatsApp only. */
export type ContactIdentityService = "phone" | "whatsapp";

export const CONTACT_IDENTITY_SERVICES = [
  "phone",
  "whatsapp",
] as const satisfies readonly ContactIdentityService[];

/** Full service list for setup and account profile handles. */
export const HANDLE_SERVICE_OPTIONS = [
  { value: "phone", label: "Phone" },
  { value: "email", label: "Email" },
  { value: "whatsapp", label: "WhatsApp" },
] as const satisfies ReadonlyArray<{ value: HandleService; label: string }>;

/** Contact drawer "Add identity" picker (labels match the handles table). */
export const CONTACT_IDENTITY_SERVICE_OPTIONS = [
  { value: "phone", label: "Text message" },
  { value: "whatsapp", label: "WhatsApp" },
] as const satisfies ReadonlyArray<{
  value: ContactIdentityService;
  label: string;
}>;

/**
 * Guess the service for a handle when the stored service is empty.
 * Emails contain `@`. Phone-like values are mostly digits.
 */
export function inferService(handle: string, service: string | null | undefined): string {
  if (service && service.trim()) return service.trim().toLowerCase();
  const h = handle.trim();
  if (h.includes("@") && !h.startsWith("@")) return "email";
  if (/^\+?\d[\d\s().-]{6,}$/.test(h)) return "phone";
  return "unknown";
}

/** Label shown in the handles table Service column. */
export function formatHandleServiceLabel(
  handle: string,
  service: string | null | undefined,
): string {
  const lower = inferService(handle, service);
  if (lower === "whatsapp") return "WhatsApp";
  if (
    lower === "phone" ||
    lower === "sms" ||
    lower === "mms" ||
    lower === "sms/mms" ||
    lower === "imessage" ||
    lower === "ios" ||
    lower === "rcs"
  ) {
    return "Text message";
  }
  if (lower === "email") return "Email";
  if (lower === "unknown") return "—";
  return lower.charAt(0).toUpperCase() + lower.slice(1);
}

/** Map a stored or guessed service onto the contact-identity picker (phone or WhatsApp). */
export function handleServiceSelectValue(
  handle: string,
  service: string | null | undefined,
): ContactIdentityService {
  return inferService(handle, service) === "whatsapp" ? "whatsapp" : "phone";
}
