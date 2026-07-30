/**
 * Vault search query parser / composer.
 *
 * Supported operators:
 *   with:  from:  to:  has:attachment  after:  before:
 *   source:  is:group  is:direct  within:  show:contact
 *   last-contact:  first-contact:
 *   "quoted phrases"  -term
 *
 * `with:` and `to:` both mean “conversation includes this person”.
 * `from:` still means they sent the message. `subject:` is accepted for
 * typed queries but is not offered in the advanced form (rare for SMS).
 *
 * `within:` limits the search to contacts on one label, ignoring whether those
 * contacts are active or inactive. `label:` is kept as an alias for older URLs.
 *
 * `first-contact:` and `last-contact:` bound a contact's overall first / last
 * message date and accept three forms:
 *   first-contact:>=2020-01-01   on or after
 *   first-contact:<2020-01-01    before
 *   first-contact:2020-01-01..2020-06-30   between
 * A bare `first-contact:2020-01-01` means “before”, matching older URLs.
 * Dates use the same YYYY / YYYY-MM-DD forms as after: and before:.
 *
 * `show:contact` renders each matching contact with their matching
 * conversations nested underneath instead of a flat conversation list.
 *
 * Trash is always excluded; a legacy `in:trash` operator is ignored.
 */

/** Inclusive lower bound / upper bound, both `YYYY-MM-DD`. */
export type DateBounds = {
  from: string | null;
  to: string | null;
};

export type ParsedSearchQuery = {
  /** Free-text terms (AND) searched via FTS. */
  terms: string[];
  /** Phrases that must appear (AND). */
  phrases: string[];
  /** Terms/phrases to exclude. */
  exclude: string[];
  from: string | null;
  to: string | null;
  subject: string | null;
  hasAttachment: boolean;
  after: string | null;
  before: string | null;
  source: string | null;
  conversationType: "group" | "individual" | null;
  /** Label whose contacts to search, active or not. */
  within: string | null;
  /** Bounds on each contact's overall last message date. */
  lastContact: DateBounds;
  /** Bounds on each contact's overall first message date. */
  firstContact: DateBounds;
  /** Group results by contact instead of listing conversations flat. */
  showContact: boolean;
};

/** Which bound(s) a date field contributes. */
export type DateFilterMode = "any" | "on-or-after" | "before" | "between";

export type DateFilterInput = {
  mode: DateFilterMode;
  from?: string;
  to?: string;
};

export type AdvancedSearchForm = {
  /** Label to search within; empty means all contacts. */
  within?: string;
  /** Name or number of a conversation participant. */
  withPerson?: string;
  hasWords?: string;
  doesntHave?: string;
  /** Message timestamp bounds. */
  date?: DateFilterInput;
  /** Contact's first message date bounds. */
  firstContact?: DateFilterInput;
  /** Contact's last message date bounds. */
  lastContact?: DateFilterInput;
  showContact?: boolean;
  conversationType?: "any" | "group" | "individual";
  source?: string;
  hasAttachment?: boolean;
};

const NO_BOUNDS: DateBounds = { from: null, to: null };

const EMPTY: ParsedSearchQuery = {
  terms: [],
  phrases: [],
  exclude: [],
  from: null,
  to: null,
  subject: null,
  hasAttachment: false,
  after: null,
  before: null,
  source: null,
  conversationType: null,
  within: null,
  lastContact: NO_BOUNDS,
  firstContact: NO_BOUNDS,
  showContact: false,
};

const OPERATOR_RE =
  /^(with|from|to|subject|has|after|before|source|is|within|label|in|show|last-contact|first-contact):(.*)$/i;

function readQuoted(s: string, start: number): { value: string; next: number } {
  let i = start;
  let phrase = "";
  while (i < s.length && s[i] !== '"') {
    phrase += s[i];
    i += 1;
  }
  if (i < s.length && s[i] === '"') i += 1;
  return { value: phrase, next: i };
}

function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let i = 0;
  const s = input.trim();
  while (i < s.length) {
    while (i < s.length && /\s/.test(s[i]!)) i += 1;
    if (i >= s.length) break;

    // Negated quoted phrase: -"bad word"
    if (s[i] === "-" && s[i + 1] === '"') {
      const { value, next } = readQuoted(s, i + 2);
      tokens.push(`-"${value}"`);
      i = next;
      continue;
    }

    // Quoted phrase: "exact phrase"
    if (s[i] === '"') {
      const { value, next } = readQuoted(s, i + 1);
      tokens.push(`"${value}"`);
      i = next;
      continue;
    }

    let tok = "";
    while (i < s.length && !/\s/.test(s[i]!)) {
      // Operator with quoted value: from:"Ann Lee"
      if (s[i] === ":" && s[i + 1] === '"') {
        tok += ':"';
        const { value, next } = readQuoted(s, i + 2);
        tok += `${value}"`;
        i = next;
        break;
      }
      tok += s[i];
      i += 1;
    }
    if (tok) tokens.push(tok);
  }
  return tokens;
}

function normalizeDate(raw: string): string | null {
  const t = raw.trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(t)) return t;
  if (/^\d{4}$/.test(t)) return `${t}-01-01`;
  return t || null;
}

/** Read `>=D`, `<D`, `D1..D2`, or a bare `D` (which means “before D”). */
function parseDateBounds(raw: string): DateBounds {
  const t = raw.trim();
  if (!t) return NO_BOUNDS;

  const range = t.match(/^(.+?)\.\.(.+)$/);
  if (range) {
    return { from: normalizeDate(range[1]!), to: normalizeDate(range[2]!) };
  }
  if (t.startsWith(">=")) return { from: normalizeDate(t.slice(2)), to: null };
  if (t.startsWith(">")) return { from: normalizeDate(t.slice(1)), to: null };
  if (t.startsWith("<=")) return { from: null, to: normalizeDate(t.slice(2)) };
  if (t.startsWith("<")) return { from: null, to: normalizeDate(t.slice(1)) };
  return { from: null, to: normalizeDate(t) };
}

export function hasDateBounds(bounds: DateBounds): boolean {
  return !!bounds.from || !!bounds.to;
}

export function parseSearchQuery(input: string): ParsedSearchQuery {
  const out: ParsedSearchQuery = {
    ...EMPTY,
    terms: [],
    phrases: [],
    exclude: [],
    lastContact: { ...NO_BOUNDS },
    firstContact: { ...NO_BOUNDS },
  };
  if (!input.trim()) return out;

  for (const raw of tokenize(input)) {
    let token = raw;
    let negated = false;
    if (token.startsWith("-") && token.length > 1) {
      negated = true;
      token = token.slice(1);
    }

    if (token.startsWith('"') && token.endsWith('"') && token.length >= 2) {
      const phrase = token.slice(1, -1).trim();
      if (!phrase) continue;
      if (negated) out.exclude.push(phrase);
      else out.phrases.push(phrase);
      continue;
    }

    const m = token.match(OPERATOR_RE);
    if (m) {
      const op = m[1]!.toLowerCase();
      const value = m[2]!.trim().replace(/^"|"$/g, "");
      if (!value && op !== "has") continue;
      switch (op) {
        case "from":
          out.from = value;
          break;
        case "with":
        case "to":
          out.to = value;
          break;
        case "subject":
          out.subject = value;
          break;
        case "has":
          if (value.toLowerCase() === "attachment") out.hasAttachment = true;
          break;
        case "after":
          out.after = normalizeDate(value);
          break;
        case "before":
          out.before = normalizeDate(value);
          break;
        case "source":
          out.source = value;
          break;
        case "is": {
          const v = value.toLowerCase();
          if (v === "group") out.conversationType = "group";
          else if (v === "direct" || v === "individual" || v === "1-1") {
            out.conversationType = "individual";
          }
          break;
        }
        case "within":
        case "label":
          out.within = value;
          break;
        case "in":
          // Legacy in:trash — trash is always excluded now.
          break;
        case "show":
          if (value.toLowerCase() === "contact") out.showContact = true;
          break;
        case "last-contact":
          out.lastContact = parseDateBounds(value);
          break;
        case "first-contact":
          out.firstContact = parseDateBounds(value);
          break;
        default:
          break;
      }
      continue;
    }

    if (negated) out.exclude.push(token);
    else out.terms.push(token);
  }

  return out;
}

/** Serialize a date field back to its operator value (`>=D`, `<D`, `D1..D2`). */
function composeDateBounds(input: DateFilterInput | undefined): string | null {
  if (!input || input.mode === "any") return null;
  const from = input.from?.trim() || "";
  const to = input.to?.trim() || "";
  if (input.mode === "on-or-after") return from ? `>=${from}` : null;
  if (input.mode === "before") return to ? `<${to}` : null;
  if (from && to) return `${from}..${to}`;
  // Partly filled "between" still narrows on the side that has a date.
  if (from) return `>=${from}`;
  if (to) return `<${to}`;
  return null;
}

/** Build a shareable query string from the advanced form fields. */
export function composeSearchQuery(form: AdvancedSearchForm): string {
  const parts: string[] = [];
  const quoteIfNeeded = (v: string) =>
    /\s/.test(v) ? `"${v.replace(/"/g, "")}"` : v;

  if (form.within?.trim()) {
    parts.push(`within:${quoteIfNeeded(form.within.trim())}`);
  }
  if (form.withPerson?.trim()) {
    parts.push(`with:${quoteIfNeeded(form.withPerson.trim())}`);
  }
  if (form.hasWords?.trim()) parts.push(form.hasWords.trim());
  if (form.doesntHave?.trim()) {
    for (const t of tokenize(form.doesntHave)) {
      if (t.startsWith('"')) parts.push(`-${t}`);
      else parts.push(`-${t.replace(/^-/, "")}`);
    }
  }

  const date = form.date;
  if (date && date.mode !== "any") {
    const from = date.from?.trim();
    const to = date.to?.trim();
    if (date.mode !== "before" && from) parts.push(`after:${from}`);
    if (date.mode !== "on-or-after" && to) parts.push(`before:${to}`);
  }

  const firstContact = composeDateBounds(form.firstContact);
  if (firstContact) parts.push(`first-contact:${firstContact}`);
  const lastContact = composeDateBounds(form.lastContact);
  if (lastContact) parts.push(`last-contact:${lastContact}`);

  if (form.conversationType === "group") parts.push("is:group");
  if (form.conversationType === "individual") parts.push("is:direct");
  if (form.source?.trim()) parts.push(`source:${form.source.trim()}`);
  if (form.hasAttachment) parts.push("has:attachment");
  if (form.showContact) parts.push("show:contact");
  return parts.join(" ");
}

export function hasSearchCriteria(q: ParsedSearchQuery): boolean {
  return (
    q.terms.length > 0 ||
    q.phrases.length > 0 ||
    q.exclude.length > 0 ||
    !!q.from ||
    !!q.to ||
    !!q.subject ||
    q.hasAttachment ||
    !!q.after ||
    !!q.before ||
    !!q.source ||
    !!q.conversationType ||
    !!q.within ||
    hasDateBounds(q.lastContact) ||
    hasDateBounds(q.firstContact)
  );
}

/**
 * Convert parsed free-text / phrases into an FTS5 MATCH expression.
 * Returns null when there is nothing to match via FTS.
 */
export function toFtsMatch(q: ParsedSearchQuery): string | null {
  const parts: string[] = [];
  for (const t of q.terms) {
    const safe = t.replace(/"/g, "");
    if (safe) parts.push(`"${safe}"`);
  }
  for (const p of q.phrases) {
    const safe = p.replace(/"/g, "");
    if (safe) parts.push(`"${safe}"`);
  }
  for (const e of q.exclude) {
    const safe = e.replace(/"/g, "");
    if (safe) parts.push(`NOT "${safe}"`);
  }
  if (q.subject?.trim()) {
    const safe = q.subject.trim().replace(/"/g, "");
    if (safe) parts.push(`subject:"${safe}"`);
  }
  if (parts.length === 0) return null;
  return parts.join(" AND ");
}
