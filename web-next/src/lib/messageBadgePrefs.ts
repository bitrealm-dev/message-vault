/** Display toggles for contact-list chrome (badges + initials + date range). */

export const SHOW_MESSAGE_BADGE_KEY = "mv-show-message-badge";
export const SHOW_GROUP_MESSAGE_BADGE_KEY = "mv-show-group-message-badge";
export const SHOW_CONTACT_INITIALS_KEY = "mv-show-contact-initials";
export const SHOW_CONTACT_DATE_RANGE_KEY = "mv-show-contact-date-range";

export type BadgeVisibility = "on" | "off";

export const DEFAULT_SHOW_MESSAGE_BADGE: BadgeVisibility = "off";
export const DEFAULT_SHOW_GROUP_MESSAGE_BADGE: BadgeVisibility = "off";
export const DEFAULT_SHOW_CONTACT_INITIALS: BadgeVisibility = "on";
/** Off by default: activity belongs in the conversations panel. */
export const DEFAULT_SHOW_CONTACT_DATE_RANGE: BadgeVisibility = "off";

export function isBadgeVisibility(
  v: string | null | undefined,
): v is BadgeVisibility {
  return v === "on" || v === "off";
}

export function readBadgeVisibility(
  key: string,
  fallback: BadgeVisibility,
): BadgeVisibility {
  if (typeof window === "undefined") return fallback;
  const raw = window.localStorage.getItem(key);
  return isBadgeVisibility(raw) ? raw : fallback;
}
