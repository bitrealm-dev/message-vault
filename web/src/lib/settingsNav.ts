/** Settings routes shown as compact tabs inside the settings content frame. */
export const SETTINGS_TABS = [
  { href: "/settings/account", label: "Account" },
  { href: "/settings/access", label: "Access" },
  { href: "/settings/storage", label: "Storage" },
  { href: "/settings/display", label: "Appearance" },
] as const;

export type SettingsTabHref = (typeof SETTINGS_TABS)[number]["href"];

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
