"use client";

import {
  DATE_CUSTOM_KEY,
  DATE_MODE_KEY,
  DEFAULT_DATE_CUSTOM,
  DEFAULT_DATE_MODE,
  DEFAULT_TIME_CUSTOM,
  DEFAULT_TIME_MODE,
  formatDateOnly,
  formatDateRange,
  formatDateTime,
  formatTimeOnly,
  isDateFormatMode,
  isTimeFormatMode,
  readStoredDateTimeFormat,
  resolveDatePattern,
  resolveTimePattern,
  TIME_CUSTOM_KEY,
  TIME_MODE_KEY,
  validateDatePattern,
  validateTimePattern,
  type DateFormatMode,
  type PatternValidation,
  type TimeFormatMode,
} from "@/lib/dateTimeFormat";
import {
  fetchServerPrefs,
  pushServerPrefs,
  reconcilePrefs,
} from "@/lib/prefsClient";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

const DATE_TIME_PREF_KEYS = [
  DATE_MODE_KEY,
  DATE_CUSTOM_KEY,
  TIME_MODE_KEY,
  TIME_CUSTOM_KEY,
] as const;

export type UseDateTimeFormatResult = {
  dateMode: DateFormatMode;
  setDateMode: (mode: DateFormatMode) => void;
  dateCustom: string;
  setDateCustom: (pattern: string) => PatternValidation;
  timeMode: TimeFormatMode;
  setTimeMode: (mode: TimeFormatMode) => void;
  timeCustom: string;
  setTimeCustom: (pattern: string) => PatternValidation;
  datePattern: string;
  timePattern: string;
  formatDate: (raw: string) => string;
  formatTime: (raw: string) => string;
  formatDateTime: (raw: string) => string;
  formatDateRange: (start: string, end: string, sep?: string) => string;
};

const DateTimeFormatContext = createContext<UseDateTimeFormatResult | null>(
  null,
);

export function DateTimeFormatProvider({ children }: { children: ReactNode }) {
  // Start from defaults on both server and client so hydration matches;
  // stored values are applied in the mount effect below.
  const [dateMode, setDateModeState] = useState<DateFormatMode>(DEFAULT_DATE_MODE);
  const [dateCustom, setDateCustomState] = useState(DEFAULT_DATE_CUSTOM);
  const [timeMode, setTimeModeState] = useState<TimeFormatMode>(DEFAULT_TIME_MODE);
  const [timeCustom, setTimeCustomState] = useState(DEFAULT_TIME_CUSTOM);

  useEffect(() => {
    const stored = readStoredDateTimeFormat();
    setDateModeState(stored.dateMode);
    setDateCustomState(stored.dateCustom);
    setTimeModeState(stored.timeMode);
    setTimeCustomState(stored.timeCustom);

    let cancelled = false;
    void fetchServerPrefs().then((serverPrefs) => {
      if (cancelled || !serverPrefs) return;
      const { values, toPush } = reconcilePrefs(
        serverPrefs,
        DATE_TIME_PREF_KEYS,
      );
      const dm = values[DATE_MODE_KEY];
      if (isDateFormatMode(dm)) setDateModeState(dm);
      const dc = values[DATE_CUSTOM_KEY];
      if (dc != null && dc.trim()) setDateCustomState(dc);
      const tm = values[TIME_MODE_KEY];
      if (isTimeFormatMode(tm)) setTimeModeState(tm);
      const tc = values[TIME_CUSTOM_KEY];
      if (tc != null && tc.trim()) setTimeCustomState(tc);
      if (Object.keys(toPush).length > 0) pushServerPrefs(toPush);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const datePattern = resolveDatePattern(dateMode, dateCustom);
  const timePattern = resolveTimePattern(timeMode, timeCustom);

  const setDateMode = useCallback((mode: DateFormatMode) => {
    setDateModeState(mode);
    window.localStorage.setItem(DATE_MODE_KEY, mode);
    pushServerPrefs({ [DATE_MODE_KEY]: mode });
  }, []);

  const setDateCustom = useCallback((pattern: string): PatternValidation => {
    const v = validateDatePattern(pattern);
    if (!v.ok) return v;
    setDateCustomState(pattern);
    window.localStorage.setItem(DATE_CUSTOM_KEY, pattern);
    pushServerPrefs({ [DATE_CUSTOM_KEY]: pattern });
    return v;
  }, []);

  const setTimeMode = useCallback((mode: TimeFormatMode) => {
    setTimeModeState(mode);
    window.localStorage.setItem(TIME_MODE_KEY, mode);
    pushServerPrefs({ [TIME_MODE_KEY]: mode });
  }, []);

  const setTimeCustom = useCallback((pattern: string): PatternValidation => {
    const v = validateTimePattern(pattern);
    if (!v.ok) return v;
    setTimeCustomState(pattern);
    window.localStorage.setItem(TIME_CUSTOM_KEY, pattern);
    pushServerPrefs({ [TIME_CUSTOM_KEY]: pattern });
    return v;
  }, []);

  const formatDateFn = useCallback(
    (raw: string) => formatDateOnly(raw, datePattern),
    [datePattern],
  );

  const formatTimeFn = useCallback(
    (raw: string) => formatTimeOnly(raw, timePattern),
    [timePattern],
  );

  const formatDateTimeFn = useCallback(
    (raw: string) => formatDateTime(raw, datePattern, timePattern),
    [datePattern, timePattern],
  );

  const formatDateRangeFn = useCallback(
    (start: string, end: string, sep?: string) =>
      formatDateRange(start, end, datePattern, sep),
    [datePattern],
  );

  const value = useMemo(
    () => ({
      dateMode,
      setDateMode,
      dateCustom,
      setDateCustom,
      timeMode,
      setTimeMode,
      timeCustom,
      setTimeCustom,
      datePattern,
      timePattern,
      formatDate: formatDateFn,
      formatTime: formatTimeFn,
      formatDateTime: formatDateTimeFn,
      formatDateRange: formatDateRangeFn,
    }),
    [
      dateMode,
      setDateMode,
      dateCustom,
      setDateCustom,
      timeMode,
      setTimeMode,
      timeCustom,
      setTimeCustom,
      datePattern,
      timePattern,
      formatDateFn,
      formatTimeFn,
      formatDateTimeFn,
      formatDateRangeFn,
    ],
  );

  return (
    <DateTimeFormatContext.Provider value={value}>
      {children}
    </DateTimeFormatContext.Provider>
  );
}

export function useDateTimeFormat(): UseDateTimeFormatResult {
  const ctx = useContext(DateTimeFormatContext);
  if (!ctx) {
    throw new Error("useDateTimeFormat must be used within DateTimeFormatProvider");
  }
  return ctx;
}

/** Safe for optional use when a component may render outside the provider in tests. */
export function useDateTimeFormatOptional(): UseDateTimeFormatResult | null {
  return useContext(DateTimeFormatContext);
}
