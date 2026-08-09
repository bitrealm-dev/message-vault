import {
  applyTheme,
  DEFAULT_MODE,
  DEFAULT_SEEDS,
  formatThemeShare,
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
} from "./theme";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

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

/** Single app-wide theme state; syncs CSS vars on `<html>`. LocalStorage only. */
export function ThemeProvider({ children }: { children: ReactNode }) {
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
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    applyTheme(mode, seeds);
  }, [hydrated, mode, seeds, prefersDark]);

  const setMode = useCallback((next: ThemeMode) => {
    setModeState(next);
    window.localStorage.setItem(THEME_MODE_KEY, next);
  }, []);

  const setSeeds = useCallback((next: ThemeSeeds) => {
    setSeedsState(next);
    window.localStorage.setItem(THEME_SEEDS_KEY, formatThemeShare(next));
  }, []);

  const patchSeed = useCallback((key: keyof ThemeSeeds, hex: string) => {
    setSeedsState((prev) => {
      const next = { ...prev, [key]: hex };
      window.localStorage.setItem(THEME_SEEDS_KEY, formatThemeShare(next));
      return next;
    });
  }, []);

  const shareString = useMemo(() => formatThemeShare(seeds), [seeds]);

  const setShareString = useCallback((raw: string) => {
    const parsed = parseThemeShare(raw);
    if (!parsed) return false;
    setSeedsState(parsed);
    window.localStorage.setItem(THEME_SEEDS_KEY, formatThemeShare(parsed));
    return true;
  }, []);

  const applyPreset = useCallback((preset: ThemePreset) => {
    setSeedsState(preset.seeds);
    window.localStorage.setItem(THEME_SEEDS_KEY, formatThemeShare(preset.seeds));
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
