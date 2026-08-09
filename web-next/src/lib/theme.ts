/** Appearance mode. `system` follows OS prefers-color-scheme. */
export type ThemeMode = "light" | "dark" | "system";

/** Resolved light/dark after applying system preference. */
export type ResolvedTheme = "light" | "dark";

/** Fastmail-style four-seed theme. */
export type ThemeSeeds = {
  lightHeader: string;
  lightAccent: string;
  darkHeader: string;
  darkAccent: string;
};

export type ThemePreset = {
  id: string;
  label: string;
  seeds: ThemeSeeds;
};

export const THEME_MODE_KEY = "mv-theme";
export const THEME_SEEDS_KEY = "mv-theme-seeds";

/** @deprecated use THEME_MODE_KEY — kept for older localStorage values */
export const THEME_STORAGE_KEY = THEME_MODE_KEY;

export const DEFAULT_MODE: ThemeMode = "dark";

/** Graphite + blue — matches the previous fixed palettes. */
export const DEFAULT_SEEDS: ThemeSeeds = {
  lightHeader: "#e6e9ee",
  lightAccent: "#2b7fff",
  darkHeader: "#222426",
  darkAccent: "#5ea1ff",
};

export const THEME_PRESETS: ThemePreset[] = [
  { id: "graphite-blue", label: "Graphite Blue", seeds: DEFAULT_SEEDS },
  {
    id: "slate-sky",
    label: "Slate Sky",
    seeds: {
      lightHeader: "#dce3ec",
      lightAccent: "#0ea5e9",
      darkHeader: "#1e293b",
      darkAccent: "#38bdf8",
    },
  },
  {
    id: "forest",
    label: "Forest",
    seeds: {
      lightHeader: "#e2ebe4",
      lightAccent: "#15803d",
      darkHeader: "#1a2420",
      darkAccent: "#4ade80",
    },
  },
  {
    id: "dusk",
    label: "Dusk",
    seeds: {
      lightHeader: "#e8e4f0",
      lightAccent: "#7c3aed",
      darkHeader: "#1f1a2e",
      darkAccent: "#a78bfa",
    },
  },
  {
    id: "rose",
    label: "Rose",
    seeds: {
      lightHeader: "#f0e6ea",
      lightAccent: "#be185d",
      darkHeader: "#2a1a22",
      darkAccent: "#f472b6",
    },
  },
  {
    id: "amber",
    label: "Amber",
    seeds: {
      lightHeader: "#f3ebe0",
      lightAccent: "#d97706",
      darkHeader: "#261e14",
      darkAccent: "#fbbf24",
    },
  },
  {
    id: "ocean",
    label: "Ocean",
    seeds: {
      lightHeader: "#dde8ec",
      lightAccent: "#0f766e",
      darkHeader: "#152528",
      darkAccent: "#2dd4bf",
    },
  },
  {
    id: "mono",
    label: "Mono",
    seeds: {
      lightHeader: "#e8e8e8",
      lightAccent: "#404040",
      darkHeader: "#2a2a2a",
      darkAccent: "#d4d4d4",
    },
  },
];

const HEX_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

export function isThemeMode(value: string | null | undefined): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

/** Legacy: treat bare light/dark as mode (not system). */
export function isResolvedTheme(
  value: string | null | undefined,
): value is ResolvedTheme {
  return value === "light" || value === "dark";
}

export function normalizeHex(raw: string): string | null {
  const t = raw.trim();
  if (!HEX_RE.test(t)) return null;
  if (t.length === 4) {
    const [, r, g, b] = t;
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  return t.toLowerCase();
}

export function formatThemeShare(seeds: ThemeSeeds): string {
  return [
    seeds.lightHeader,
    seeds.lightAccent,
    seeds.darkHeader,
    seeds.darkAccent,
  ]
    .map((h) => normalizeHex(h) ?? h)
    .join(",");
}

export function parseThemeShare(raw: string): ThemeSeeds | null {
  const parts = raw
    .split(/[,\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length !== 4) return null;
  const hexes = parts.map(normalizeHex);
  if (hexes.some((h) => h == null)) return null;
  return {
    lightHeader: hexes[0]!,
    lightAccent: hexes[1]!,
    darkHeader: hexes[2]!,
    darkAccent: hexes[3]!,
  };
}

export function parseStoredSeeds(raw: string | null): ThemeSeeds | null {
  if (!raw) return null;
  const asShare = parseThemeShare(raw);
  if (asShare) return asShare;
  try {
    const obj = JSON.parse(raw) as Partial<ThemeSeeds>;
    const seeds: ThemeSeeds = {
      lightHeader: normalizeHex(obj.lightHeader ?? "") ?? "",
      lightAccent: normalizeHex(obj.lightAccent ?? "") ?? "",
      darkHeader: normalizeHex(obj.darkHeader ?? "") ?? "",
      darkAccent: normalizeHex(obj.darkAccent ?? "") ?? "",
    };
    if (
      !seeds.lightHeader ||
      !seeds.lightAccent ||
      !seeds.darkHeader ||
      !seeds.darkAccent
    ) {
      return null;
    }
    return seeds;
  } catch {
    return null;
  }
}

export function prefersDarkScheme(): boolean {
  if (typeof window === "undefined") return true;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function resolveMode(
  mode: ThemeMode,
  prefersDark = prefersDarkScheme(),
): ResolvedTheme {
  if (mode === "system") return prefersDark ? "dark" : "light";
  return mode;
}

export function activeSeeds(
  seeds: ThemeSeeds,
  resolved: ResolvedTheme,
): { header: string; accent: string } {
  return resolved === "dark"
    ? { header: seeds.darkHeader, accent: seeds.darkAccent }
    : { header: seeds.lightHeader, accent: seeds.lightAccent };
}

/** Apply mode + seeds to `<html>` (`data-theme`, `--header`, `--accent`). */
export function applyTheme(mode: ThemeMode, seeds: ThemeSeeds): ResolvedTheme {
  const resolved = resolveMode(mode);
  const { header, accent } = activeSeeds(seeds, resolved);
  const root = document.documentElement;
  root.setAttribute("data-theme", resolved);
  root.style.setProperty("--header", header);
  root.style.setProperty("--accent", accent);
  return resolved;
}

export function readStoredMode(): ThemeMode {
  if (typeof window === "undefined") return DEFAULT_MODE;
  const raw = window.localStorage.getItem(THEME_MODE_KEY);
  if (isThemeMode(raw)) return raw;
  // Migrate legacy light/dark-only storage
  if (isResolvedTheme(raw)) return raw;
  return DEFAULT_MODE;
}

export function readStoredSeeds(): ThemeSeeds {
  if (typeof window === "undefined") return DEFAULT_SEEDS;
  return parseStoredSeeds(window.localStorage.getItem(THEME_SEEDS_KEY)) ?? DEFAULT_SEEDS;
}

/**
 * Inline FOUC boot: set data-theme + --header/--accent from localStorage
 * before first paint. Surfaces derive in CSS via color-mix.
 */
export const THEME_BOOT_SCRIPT = `(function(){try{var mk=${JSON.stringify(THEME_MODE_KEY)};var sk=${JSON.stringify(THEME_SEEDS_KEY)};var defH=${JSON.stringify(DEFAULT_SEEDS.darkHeader)};var defA=${JSON.stringify(DEFAULT_SEEDS.darkAccent)};var defLH=${JSON.stringify(DEFAULT_SEEDS.lightHeader)};var defLA=${JSON.stringify(DEFAULT_SEEDS.lightAccent)};var mode=localStorage.getItem(mk);if(mode!=="light"&&mode!=="dark"&&mode!=="system")mode="dark";var dark=mode==="dark"||(mode==="system"&&window.matchMedia("(prefers-color-scheme: dark)").matches);var seeds=null;try{var raw=localStorage.getItem(sk);if(raw){if(raw.charAt(0)==="{"){seeds=JSON.parse(raw);}else{var p=raw.split(/[,\\s]+/).filter(Boolean);if(p.length===4)seeds={lightHeader:p[0],lightAccent:p[1],darkHeader:p[2],darkAccent:p[3]};}}}catch(e){}var h=dark?(seeds&&seeds.darkHeader||defH):(seeds&&seeds.lightHeader||defLH);var a=dark?(seeds&&seeds.darkAccent||defA):(seeds&&seeds.lightAccent||defLA);var r=document.documentElement;r.setAttribute("data-theme",dark?"dark":"light");r.style.setProperty("--header",h);r.style.setProperty("--accent",a);}catch(e){var r2=document.documentElement;r2.setAttribute("data-theme","dark");r2.style.setProperty("--header",${JSON.stringify(DEFAULT_SEEDS.darkHeader)});r2.style.setProperty("--accent",${JSON.stringify(DEFAULT_SEEDS.darkAccent)});}})();`;
