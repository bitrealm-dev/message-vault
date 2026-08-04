/** Settings routes shown as compact tabs inside the settings content frame. */
export const SETTINGS_TABS = [
  { href: "/settings/account", label: "Account" },
  { href: "/settings/access", label: "Access" },
  { href: "/settings/storage", label: "Storage" },
  { href: "/settings/display", label: "Appearance" },
] as const;

export type SettingsTabHref = (typeof SETTINGS_TABS)[number]["href"];

const SETTINGS_RETURN_TO_PARAM = "returnTo";
const DEFAULT_SETTINGS_RETURN_TO = "/all";

/** True when the app is on any settings route. */
export function isSettingsPath(path: string): boolean {
  return path === "/settings" || path.startsWith("/settings/");
}

/** True when a settings tab should appear selected for the current pathname. */
export function settingsTabActive(pathname: string, href: SettingsTabHref): boolean {
  if (pathname === href) return true;
  // `/settings` redirects to account; treat it as Account while the hop resolves.
  return href === "/settings/account" && pathname === "/settings";
}

/** Current in-app URL (path + query) for returnTo capture. */
export function currentAppUrl(pathname: string, search: string): string {
  return search ? `${pathname}?${search}` : pathname;
}

/** True when a decoded returnTo target is a safe internal non-settings path. */
export function isValidSettingsReturnTo(path: string): boolean {
  if (!path.startsWith("/")) return false;
  if (path.startsWith("//")) return false;
  if (path.includes("\\")) return false;
  if (path.includes("://")) return false;
  const pathOnly = path.split("?")[0]?.split("#")[0] ?? path;
  return !isSettingsPath(pathOnly);
}

/** Resolve returnTo from settings query params; fallback `/all`. */
export function resolveSettingsReturnTo(raw: string | null | undefined): string {
  if (!raw) return DEFAULT_SETTINGS_RETURN_TO;
  try {
    const decoded = decodeURIComponent(raw);
    return isValidSettingsReturnTo(decoded) ? decoded : DEFAULT_SETTINGS_RETURN_TO;
  } catch {
    return DEFAULT_SETTINGS_RETURN_TO;
  }
}

/** Settings account entry URL, optionally carrying a returnTo destination. */
export function settingsAccountHref(returnTo?: string | null): string {
  const base = "/settings/account";
  if (!returnTo) return base;
  if (!isValidSettingsReturnTo(returnTo)) return base;
  return `${base}?${SETTINGS_RETURN_TO_PARAM}=${encodeURIComponent(returnTo)}`;
}

/** Append returnTo to a settings tab href when the query param is valid. */
export function settingsTabHref(
  tabHref: SettingsTabHref,
  returnToRaw: string | null | undefined,
): string {
  if (!returnToRaw) return tabHref;
  try {
    const decoded = decodeURIComponent(returnToRaw);
    if (!isValidSettingsReturnTo(decoded)) return tabHref;
    return `${tabHref}?${SETTINGS_RETURN_TO_PARAM}=${encodeURIComponent(decoded)}`;
  } catch {
    return tabHref;
  }
}

/** Sidebar / unlock-vault settings link from the current browse location. */
export function settingsLinkFromLocation(
  pathname: string,
  searchParams: { get(name: string): string | null; toString(): string },
): string {
  if (isSettingsPath(pathname)) {
    return settingsAccountHref(
      resolveSettingsReturnTo(searchParams.get(SETTINGS_RETURN_TO_PARAM)),
    );
  }
  return settingsAccountHref(currentAppUrl(pathname, searchParams.toString()));
}
