/** Normalize stored hints (old `**********` or new `..`) to `mv-api-xx..yy`. */
import { formatUnixDate } from "../../lib/formatDate";

export function displayKeyHint(hint: string | null | undefined): string {
  const raw = (hint ?? "").trim();
  if (!raw) return "mv-api-..";
  if (/^(mv-api-|mv-app-).{2}\.\..{2}$/.test(raw)) return raw;
  const stars = raw.match(/^(mv-api-|mv-app-)(.{2}).*\*{2,}(.{2})$/);
  if (stars) return `${stars[1]}${stars[2]}..${stars[3]}`;
  return raw;
}

export function formatTokenDate(secs: string | null | undefined): string {
  return formatUnixDate(secs);
}

export function scopesLabel(scopes: string): string {
  switch (scopes) {
    case "import":
      return "Import";
    case "export":
      return "Export";
    case "both":
      return "Import / Export";
    default:
      return scopes;
  }
}

export type ApiTokenItem = {
  id: string;
  label: string;
  scopes: string;
  /** Masked secret, e.g. `mv-api-Sd..mE`. */
  token_hint: string;
  created_at: string;
  /** Unix seconds string, or null/absent if never used. */
  last_accessed_at?: string | null;
};

export const thClass =
  "px-3 py-2 text-left text-[0.75rem] font-bold text-muted";
export const tdClass = "px-3 py-2 text-[0.75rem] text-text align-middle";
export const tdMuted = "px-3 py-2 text-[0.75rem] text-muted align-middle";
export const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !p-0 !leading-none !text-muted";
