/** Display date/time formats (strftime-shaped). Device localStorage. */

export type DateFormatMode = "ymd" | "custom";
export type TimeFormatMode = "24h" | "12h" | "custom";

export const DATE_MODE_KEY = "mv-date-mode";
export const DATE_CUSTOM_KEY = "mv-date-custom";
export const TIME_MODE_KEY = "mv-time-mode";
export const TIME_CUSTOM_KEY = "mv-time-custom";

/** Legacy key from group list date styles — migrated once. */
export const LEGACY_GROUP_DATE_FORMAT_KEY = "mv-group-date-format";

export const DEFAULT_DATE_MODE: DateFormatMode = "ymd";
export const DEFAULT_TIME_MODE: TimeFormatMode = "24h";
export const DEFAULT_DATE_CUSTOM = "%Y-%m-%d";
export const DEFAULT_TIME_CUSTOM = "%H:%M:%S";

export const DATE_PRESET_PATTERN = "%Y-%m-%d";
export const TIME_24H_PATTERN = "%H:%M:%S";
export const TIME_12H_PATTERN = "%I:%M:%S %p";

const DATE_TOKENS = new Set([
  "a",
  "A",
  "b",
  "B",
  "d",
  "e",
  "j",
  "m",
  "U",
  "W",
  "w",
  "x",
  "y",
  "Y",
  "%",
]);

const TIME_TOKENS = new Set([
  "H",
  "I",
  "l",
  "M",
  "N",
  "P",
  "p",
  "S",
  "X",
  "Z",
  "%",
]);

const WEEKDAYS_SHORT = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;
const WEEKDAYS_LONG = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;
const MONTHS_SHORT = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;
const MONTHS_LONG = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

export type ParsedInstant = {
  year: number;
  month: number; // 1-12
  day: number;
  hour: number;
  minute: number;
  second: number;
  nano: number;
  /** JS Date for weekday / day-of-year / week calcs (local). */
  date: Date;
};

export function isDateFormatMode(
  v: string | null | undefined,
): v is DateFormatMode {
  return v === "ymd" || v === "custom";
}

export function isTimeFormatMode(
  v: string | null | undefined,
): v is TimeFormatMode {
  return v === "24h" || v === "12h" || v === "custom";
}

export function resolveDatePattern(
  mode: DateFormatMode,
  custom: string,
): string {
  if (mode === "custom" && custom.trim()) return custom;
  return DATE_PRESET_PATTERN;
}

export function resolveTimePattern(
  mode: TimeFormatMode,
  custom: string,
): string {
  if (mode === "custom" && custom.trim()) return custom;
  if (mode === "12h") return TIME_12H_PATTERN;
  return TIME_24H_PATTERN;
}

function scanTokens(pattern: string): string[] {
  const tokens: string[] = [];
  for (let i = 0; i < pattern.length; i++) {
    if (pattern[i] !== "%") continue;
    const next = pattern[i + 1];
    if (!next) {
      tokens.push("");
      break;
    }
    tokens.push(next);
    i++;
  }
  return tokens;
}

export type PatternValidation = { ok: true } | { ok: false; error: string };

export function validateDatePattern(pattern: string): PatternValidation {
  if (!pattern.trim()) return { ok: false, error: "Enter a date format." };
  for (const t of scanTokens(pattern)) {
    if (!t) return { ok: false, error: "A format can’t end with %." };
    if (t === "c") {
      return { ok: false, error: "%c isn’t supported here." };
    }
    if (!DATE_TOKENS.has(t)) {
      if (TIME_TOKENS.has(t)) {
        return { ok: false, error: `%${t} is for times. Use it under Time.` };
      }
      return { ok: false, error: `Unrecognized format code: %${t}` };
    }
  }
  return { ok: true };
}

export function validateTimePattern(pattern: string): PatternValidation {
  if (!pattern.trim()) return { ok: false, error: "Enter a time format." };
  for (const t of scanTokens(pattern)) {
    if (!t) return { ok: false, error: "A format can’t end with %." };
    if (t === "c") {
      return { ok: false, error: "%c isn’t supported here." };
    }
    if (!TIME_TOKENS.has(t)) {
      if (DATE_TOKENS.has(t)) {
        return { ok: false, error: `%${t} is for dates. Use it under Date.` };
      }
      return { ok: false, error: `Unrecognized format code: %${t}` };
    }
  }
  return { ok: true };
}

/** Parse vault ISO date or timestamp into local calendar fields. */
export function parseInstant(raw: string): ParsedInstant | null {
  const m =
    /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?)?/.exec(
      raw.trim(),
    );
  if (!m) return null;
  const year = Number(m[1]);
  const month = Number(m[2]);
  const day = Number(m[3]);
  const hour = m[4] != null ? Number(m[4]) : 0;
  const minute = m[5] != null ? Number(m[5]) : 0;
  const second = m[6] != null ? Number(m[6]) : 0;
  let nano = 0;
  if (m[7]) {
    const frac = m[7].padEnd(9, "0").slice(0, 9);
    nano = Number(frac);
  }
  const date = new Date(year, month - 1, day, hour, minute, second);
  if (Number.isNaN(date.getTime())) return null;
  return { year, month, day, hour, minute, second, nano, date };
}

function dayOfYear(d: Date): number {
  const start = new Date(d.getFullYear(), 0, 0);
  const diff = d.getTime() - start.getTime();
  return Math.floor(diff / 86_400_000);
}

/** Week number Sunday-based (%U). */
function weekNumberSunday(d: Date): number {
  const yearStart = new Date(d.getFullYear(), 0, 1);
  const dayOfYearNum = dayOfYear(d);
  const startDow = yearStart.getDay(); // 0=Sun
  return Math.floor((dayOfYearNum + startDow - 1) / 7);
}

/** Week number Monday-based (%W). */
function weekNumberMonday(d: Date): number {
  const yearStart = new Date(d.getFullYear(), 0, 1);
  const dayOfYearNum = dayOfYear(d);
  const startDow = (yearStart.getDay() + 6) % 7; // 0=Mon
  return Math.floor((dayOfYearNum + startDow - 1) / 7);
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function pad3(n: number): string {
  return String(n).padStart(3, "0");
}

function pad9(n: number): string {
  return String(n).padStart(9, "0");
}

export function strftime(instant: ParsedInstant, pattern: string): string {
  const { year, month, day, hour, minute, second, nano, date } = instant;
  let out = "";
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i]!;
    if (ch !== "%") {
      out += ch;
      continue;
    }
    const t = pattern[++i];
    if (t == null) {
      out += "%";
      break;
    }
    switch (t) {
      case "%":
        out += "%";
        break;
      case "a":
        out += WEEKDAYS_SHORT[date.getDay()];
        break;
      case "A":
        out += WEEKDAYS_LONG[date.getDay()];
        break;
      case "b":
        out += MONTHS_SHORT[month - 1];
        break;
      case "B":
        out += MONTHS_LONG[month - 1];
        break;
      case "d":
        out += pad2(day);
        break;
      case "e":
        out += String(day);
        break;
      case "j":
        out += pad3(dayOfYear(date));
        break;
      case "m":
        out += pad2(month);
        break;
      case "U":
        out += pad2(weekNumberSunday(date));
        break;
      case "W":
        out += pad2(weekNumberMonday(date));
        break;
      case "w":
        out += String(date.getDay());
        break;
      case "x":
        out += `${pad2(month)}/${pad2(day)}/${year}`;
        break;
      case "y":
        out += pad2(year % 100);
        break;
      case "Y":
        out += String(year);
        break;
      case "H":
        out += pad2(hour);
        break;
      case "I": {
        const h12 = hour % 12 || 12;
        out += pad2(h12);
        break;
      }
      case "l": {
        const h12 = hour % 12 || 12;
        out += String(h12);
        break;
      }
      case "M":
        out += pad2(minute);
        break;
      case "N":
        out += pad9(nano);
        break;
      case "P":
        out += hour < 12 ? "am" : "pm";
        break;
      case "p":
        out += hour < 12 ? "AM" : "PM";
        break;
      case "S":
        out += pad2(second);
        break;
      case "X":
        out += `${pad2(hour)}:${pad2(minute)}:${pad2(second)}`;
        break;
      case "Z":
        out += "";
        break;
      case "c":
        out += `${WEEKDAYS_SHORT[date.getDay()]} ${MONTHS_SHORT[month - 1]} ${pad2(day)} ${pad2(hour)}:${pad2(minute)}:${pad2(second)} ${year}`;
        break;
      default:
        out += `%${t}`;
        break;
    }
  }
  return out;
}

export function formatDateOnly(raw: string, datePattern: string): string {
  const instant = parseInstant(raw);
  if (!instant) return raw;
  return strftime(instant, datePattern);
}

export function formatTimeOnly(raw: string, timePattern: string): string {
  const instant = parseInstant(raw);
  if (!instant) return raw;
  return strftime(instant, timePattern);
}

/** Stable YYYY-MM-DD key for day-boundary comparisons in message lists. */
export function calendarDayKey(raw: string): string | null {
  const instant = parseInstant(raw);
  if (!instant) return null;
  const y = String(instant.year);
  const m = String(instant.month).padStart(2, "0");
  const d = String(instant.day).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

export function formatDateTime(
  raw: string,
  datePattern: string,
  timePattern: string,
): string {
  const instant = parseInstant(raw);
  if (!instant) return raw;
  const d = strftime(instant, datePattern);
  const t = strftime(instant, timePattern);
  if (!t.trim()) return d;
  return `${d} ${t}`;
}

export function formatDateRange(
  start: string,
  end: string,
  datePattern: string,
  sep = " — ",
): string {
  const a = formatDateOnly(start, datePattern);
  if (end === start) return a;
  return `${a}${sep}${formatDateOnly(end, datePattern)}`;
}

export type StoredDateTimeFormat = {
  dateMode: DateFormatMode;
  dateCustom: string;
  timeMode: TimeFormatMode;
  timeCustom: string;
};

export function migrateLegacyGroupDateFormat(): Partial<StoredDateTimeFormat> | null {
  if (typeof window === "undefined") return null;
  if (window.localStorage.getItem(DATE_MODE_KEY) != null) return null;
  const legacy = window.localStorage.getItem(LEGACY_GROUP_DATE_FORMAT_KEY);
  if (!legacy) return null;
  if (legacy === "mon-d") {
    return { dateMode: "custom", dateCustom: "%b %d, %Y" };
  }
  if (legacy === "d-mon") {
    return { dateMode: "custom", dateCustom: "%d %b %Y" };
  }
  // "md" or unknown → default ymd
  return { dateMode: "ymd", dateCustom: DEFAULT_DATE_CUSTOM };
}

export function readStoredDateTimeFormat(): StoredDateTimeFormat {
  if (typeof window === "undefined") {
    return {
      dateMode: DEFAULT_DATE_MODE,
      dateCustom: DEFAULT_DATE_CUSTOM,
      timeMode: DEFAULT_TIME_MODE,
      timeCustom: DEFAULT_TIME_CUSTOM,
    };
  }

  const migrated = migrateLegacyGroupDateFormat();
  let dateMode = DEFAULT_DATE_MODE;
  let dateCustom = DEFAULT_DATE_CUSTOM;
  let timeMode = DEFAULT_TIME_MODE;
  let timeCustom = DEFAULT_TIME_CUSTOM;

  if (migrated) {
    if (migrated.dateMode) dateMode = migrated.dateMode;
    if (migrated.dateCustom) dateCustom = migrated.dateCustom;
    window.localStorage.setItem(DATE_MODE_KEY, dateMode);
    window.localStorage.setItem(DATE_CUSTOM_KEY, dateCustom);
  } else {
    const dm = window.localStorage.getItem(DATE_MODE_KEY);
    if (isDateFormatMode(dm)) dateMode = dm;
    const dc = window.localStorage.getItem(DATE_CUSTOM_KEY);
    if (dc != null && dc.trim()) dateCustom = dc;
  }

  const tm = window.localStorage.getItem(TIME_MODE_KEY);
  if (isTimeFormatMode(tm)) timeMode = tm;
  const tc = window.localStorage.getItem(TIME_CUSTOM_KEY);
  if (tc != null && tc.trim()) timeCustom = tc;

  return { dateMode, dateCustom, timeMode, timeCustom };
}

/** Syntax reference rows for the Display settings panel. */
export const DATE_SYNTAX_ROWS: { token: string; desc: string }[] = [
  { token: "%a", desc: 'Abbreviated weekday ("Sun")' },
  { token: "%A", desc: 'Full weekday ("Sunday")' },
  { token: "%b", desc: 'Abbreviated month ("Jan")' },
  { token: "%B", desc: 'Full month ("January")' },
  { token: "%d", desc: "Day of month (01..31)" },
  { token: "%e", desc: "Day of month, no leading zero (1..31)" },
  { token: "%j", desc: "Day of year (001..366)" },
  { token: "%m", desc: "Month (01..12)" },
  { token: "%U", desc: "Week number, Sunday-based (00..53)" },
  { token: "%W", desc: "Week number, Monday-based (00..53)" },
  { token: "%w", desc: "Day of week (0=Sunday..6)" },
  { token: "%x", desc: "Preferred date representation" },
  { token: "%y", desc: "Year without century (00..99)" },
  { token: "%Y", desc: "Year with century" },
  { token: "%%", desc: 'Literal "%" character' },
];

export const TIME_SYNTAX_ROWS: { token: string; desc: string }[] = [
  { token: "%H", desc: "Hour, 24-hour clock (00..23)" },
  { token: "%I", desc: "Hour, 12-hour clock (01..12)" },
  { token: "%l", desc: "Hour, 12-hour clock, no leading zero (1..12)" },
  { token: "%M", desc: "Minute (00..59)" },
  { token: "%N", desc: "Fraction of a second" },
  { token: "%P", desc: 'Meridian, lowercase ("am" or "pm")' },
  { token: "%p", desc: 'Meridian, uppercase ("AM" or "PM")' },
  { token: "%S", desc: "Second (00..60)" },
  { token: "%X", desc: "Preferred time representation" },
  { token: "%Z", desc: "Timezone name" },
  { token: "%%", desc: 'Literal "%" character' },
];
