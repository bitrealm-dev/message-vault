/**
 * Vault search query parser / composer.
 *
 * Supported operators:
 *   with:  from:  to:  has:attachment  after:  before:
 *   source:  is:group  is:direct  label:  in:trash
 *   last-contact:  first-contact:
 *   "quoted phrases"  -term
 *
 * `with:` and `to:` both mean “conversation includes this person”.
 * `from:` still means they sent the message. `subject:` is accepted for
 * typed queries but is not offered in the advanced form (rare for SMS).
 * `last-contact:` matches conversations whose last message is on or before
 * that date (e.g. last-contact:2024-01-01). `first-contact:` matches
 * conversations whose first message is on or before that date
 * (e.g. first-contact:2019-06-01). Dates use the same YYYY / YYYY-MM-DD
 * forms as after: and before:.
 */

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
  label: string | null;
  includeTrash: boolean;
  /** Last message on or before this date (YYYY-MM-DD). */
  lastContact: string | null;
  /** First message on or before this date (YYYY-MM-DD). */
  firstContact: string | null;
};

export type AdvancedSearchForm = {
  /** Name or number of a conversation participant. */
  withPerson?: string;
  hasWords?: string;
  doesntHave?: string;
  after?: string;
  before?: string;
  source?: string;
  conversationType?: "any" | "group" | "individual";
  hasAttachment?: boolean;
  label?: string;
  includeTrash?: boolean;
  /** Last message on or before this date → last-contact: */
  lastContact?: string;
  /** First message on or before this date → first-contact: */
  firstContact?: string;
};

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
  label: null,
  includeTrash: false,
  lastContact: null,
  firstContact: null,
};

const OPERATOR_RE =
  /^(with|from|to|subject|has|after|before|source|is|label|in|last-contact|first-contact):(.*)$/i;

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

export function parseSearchQuery(input: string): ParsedSearchQuery {
  const out: ParsedSearchQuery = {
    ...EMPTY,
    terms: [],
    phrases: [],
    exclude: [],
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
        case "label":
          out.label = value;
          break;
        case "in":
          if (value.toLowerCase() === "trash") out.includeTrash = true;
          break;
        case "last-contact":
          out.lastContact = normalizeDate(value);
          break;
        case "first-contact":
          out.firstContact = normalizeDate(value);
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

/** Build a shareable query string from the advanced form fields. */
export function composeSearchQuery(form: AdvancedSearchForm): string {
  const parts: string[] = [];
  const quoteIfNeeded = (v: string) =>
    /\s/.test(v) ? `"${v.replace(/"/g, "")}"` : v;

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
  if (form.after?.trim()) parts.push(`after:${form.after.trim()}`);
  if (form.before?.trim()) parts.push(`before:${form.before.trim()}`);
  if (form.lastContact?.trim()) {
    parts.push(`last-contact:${form.lastContact.trim()}`);
  }
  if (form.firstContact?.trim()) {
    parts.push(`first-contact:${form.firstContact.trim()}`);
  }
  if (form.source?.trim()) parts.push(`source:${form.source.trim()}`);
  if (form.conversationType === "group") parts.push("is:group");
  if (form.conversationType === "individual") parts.push("is:direct");
  if (form.hasAttachment) parts.push("has:attachment");
  if (form.label?.trim()) {
    parts.push(`label:${quoteIfNeeded(form.label.trim())}`);
  }
  if (form.includeTrash) parts.push("in:trash");
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
    !!q.label ||
    !!q.lastContact ||
    !!q.firstContact
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
