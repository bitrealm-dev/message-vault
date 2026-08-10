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

/** Ocean Depths — theme-factory default. */
export const DEFAULT_SEEDS: ThemeSeeds = {
  lightHeader: "#f1faee",
  lightAccent: "#2d8b8b",
  darkHeader: "#1a2332",
  darkAccent: "#a8dadc",
};

export const THEME_PRESETS: ThemePreset[] = [
  { id: "ocean-depths", label: "Ocean Depths", seeds: DEFAULT_SEEDS },
  {
    id: "graphite-blue",
    label: "Graphite Blue",
    seeds: {
      lightHeader: "#e6e9ee",
      lightAccent: "#2b7fff",
      darkHeader: "#222426",
      darkAccent: "#5ea1ff",
    },
  },
  {
    id: "light",
    label: "Light",
    seeds: {
      lightHeader: "#f0f2f5",
      lightAccent: "#2563eb",
      darkHeader: "#2c3036",
      darkAccent: "#6ba3ff",
    },
  },
  {
    id: "dark",
    label: "Dark",
    seeds: {
      lightHeader: "#e8eaed",
      lightAccent: "#3b82f6",
      darkHeader: "#141618",
      darkAccent: "#5ea1ff",
    },
  },
  {
    id: "sunset-boulevard",
    label: "Sunset Boulevard",
    seeds: {
      lightHeader: "#e9c46a",
      lightAccent: "#e76f51",
      darkHeader: "#264653",
      darkAccent: "#f4a261",
    },
  },
  {
    id: "forest-canopy",
    label: "Forest Canopy",
    seeds: {
      lightHeader: "#faf9f6",
      lightAccent: "#7d8471",
      darkHeader: "#2d4a2b",
      darkAccent: "#a4ac86",
    },
  },
  {
    id: "modern-minimalist",
    label: "Modern Minimalist",
    seeds: {
      lightHeader: "#d3d3d3",
      lightAccent: "#708090",
      darkHeader: "#36454f",
      // Light Gray darkened so white sent-text stays readable
      darkAccent: "#9aa8b5",
    },
  },
  {
    id: "golden-hour",
    label: "Golden Hour",
    seeds: {
      lightHeader: "#d4b896",
      lightAccent: "#c1666b",
      darkHeader: "#4a403a",
      darkAccent: "#f4a900",
    },
  },
  {
    id: "arctic-frost",
    label: "Arctic Frost",
    seeds: {
      lightHeader: "#fafafa",
      lightAccent: "#4a6fa5",
      darkHeader: "#4a6fa5",
      // Ice Blue darkened toward Steel Blue for sent-text contrast
      darkAccent: "#5a7fb5",
    },
  },
  {
    id: "desert-rose",
    label: "Desert Rose",
    seeds: {
      lightHeader: "#e8d5c4",
      lightAccent: "#b87d6d",
      darkHeader: "#5d2e46",
      darkAccent: "#d4a5a5",
    },
  },
  {
    id: "tech-innovation",
    label: "Tech Innovation",
    seeds: {
      lightHeader: "#ffffff",
      lightAccent: "#0066ff",
      darkHeader: "#1e1e1e",
      // Neon Cyan mixed toward Electric Blue for sent-text contrast
      darkAccent: "#0088bb",
    },
  },
  {
    id: "botanical-garden",
    label: "Botanical Garden",
    seeds: {
      lightHeader: "#f5f3ed",
      lightAccent: "#b7472a",
      darkHeader: "#4a7c59",
      darkAccent: "#f9a620",
    },
  },
  {
    id: "midnight-galaxy",
    label: "Midnight Galaxy",
    seeds: {
      lightHeader: "#e6e6fa",
      lightAccent: "#4a4e8f",
      darkHeader: "#2b1e3e",
      darkAccent: "#a490c2",
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
