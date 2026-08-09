"use client";

import {
  fetchServerPrefs,
  pushServerPrefs,
  reconcilePrefs,
} from "@/lib/prefsClient";
import {
  applyTheme,
  DEFAULT_MODE,
  DEFAULT_SEEDS,
  formatThemeShare,
  isThemeMode,
  parseStoredSeeds,
  parseThemeShare,
  readStoredMode,
  readStoredSeeds,
  resolveMode,
  THEME_MODE_KEY,
  THEME_PRESETS,
  THEME_SEEDS_KEY,
  type ResolvedTheme,
  type ThemeMode,
  type ThemePreset,
  type ThemeSeeds,
} from "@/lib/theme";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

const THEME_PREF_KEYS = [THEME_MODE_KEY, THEME_SEEDS_KEY] as const;

export type UseThemeResult = {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
  seeds: ThemeSeeds;
  setSeeds: (seeds: ThemeSeeds) => void;
  patchSeed: (key: keyof ThemeSeeds, hex: string) => void;
  shareString: string;
  setShareString: (raw: string) => boolean;
  applyPreset: (preset: ThemePreset) => void;
  resolvedMode: ResolvedTheme;
  presets: ThemePreset[];
};

const ThemeContext = createContext<UseThemeResult | null>(null);

/** Single app-wide theme state; syncs CSS vars on `<html>`. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  // Start from defaults on both server and client so hydration matches;
  // stored values are applied in the mount effect below. The page itself
  // is themed pre-hydration by THEME_BOOT_SCRIPT, so no visual flash.
  const [mode, setModeState] = useState<ThemeMode>(DEFAULT_MODE);
  const [seeds, setSeedsState] = useState<ThemeSeeds>(DEFAULT_SEEDS);
  const [prefersDark, setPrefersDark] = useState(true);
  // Don't applyTheme until stored values load, so we never stomp the
  // boot-script theme with defaults for a frame.
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    setModeState(readStoredMode());
    setSeedsState(readStoredSeeds());
    setHydrated(true);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    setPrefersDark(mq.matches);
    const onChange = () => setPrefersDark(mq.matches);
    mq.addEventListener("change", onChange);

    let cancelled = false;
    void fetchServerPrefs().then((serverPrefs) => {
      if (cancelled || !serverPrefs) return;
      const { values, toPush } = reconcilePrefs(serverPrefs, THEME_PREF_KEYS);
      const modeVal = values[THEME_MODE_KEY];
      if (isThemeMode(modeVal)) setModeState(modeVal);
      const seedsVal = values[THEME_SEEDS_KEY];
      if (seedsVal != null) {
        const parsed = parseStoredSeeds(seedsVal);
        if (parsed) setSeedsState(parsed);
      }
      if (Object.keys(toPush).length > 0) pushServerPrefs(toPush);
    });

    return () => {
      cancelled = true;
      mq.removeEventListener("change", onChange);
    };
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    applyTheme(mode, seeds);
  }, [hydrated, mode, seeds, prefersDark]);

  const setMode = useCallback((next: ThemeMode) => {
    setModeState(next);
    window.localStorage.setItem(THEME_MODE_KEY, next);
    pushServerPrefs({ [THEME_MODE_KEY]: next });
  }, []);

  const setSeeds = useCallback((next: ThemeSeeds) => {
    setSeedsState(next);
    const share = formatThemeShare(next);
    window.localStorage.setItem(THEME_SEEDS_KEY, share);
    pushServerPrefs({ [THEME_SEEDS_KEY]: share });
  }, []);

  const patchSeed = useCallback((key: keyof ThemeSeeds, hex: string) => {
    setSeedsState((prev) => {
      const next = { ...prev, [key]: hex };
      const share = formatThemeShare(next);
      window.localStorage.setItem(THEME_SEEDS_KEY, share);
      pushServerPrefs({ [THEME_SEEDS_KEY]: share });
      return next;
    });
  }, []);

  const shareString = useMemo(() => formatThemeShare(seeds), [seeds]);

  const setShareString = useCallback((raw: string) => {
    const parsed = parseThemeShare(raw);
    if (!parsed) return false;
    setSeedsState(parsed);
    const share = formatThemeShare(parsed);
    window.localStorage.setItem(THEME_SEEDS_KEY, share);
    pushServerPrefs({ [THEME_SEEDS_KEY]: share });
    return true;
  }, []);

  const applyPreset = useCallback((preset: ThemePreset) => {
    setSeedsState(preset.seeds);
    const share = formatThemeShare(preset.seeds);
    window.localStorage.setItem(THEME_SEEDS_KEY, share);
    pushServerPrefs({ [THEME_SEEDS_KEY]: share });
  }, []);

  const resolvedMode = resolveMode(mode, prefersDark);

  const value = useMemo(
    () => ({
      mode,
      setMode,
      seeds,
      setSeeds,
      patchSeed,
      shareString,
      setShareString,
      applyPreset,
      resolvedMode,
      presets: THEME_PRESETS,
    }),
    [
      mode,
      setMode,
      seeds,
      setSeeds,
      patchSeed,
      shareString,
      setShareString,
      applyPreset,
      resolvedMode,
    ],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme(): UseThemeResult {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within ThemeProvider");
  }
  return ctx;
}
