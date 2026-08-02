/**
 * Vault search query parser / composer.
 *
 * Supported operators:
 *   search:contacts  handle:  first:  last:  phone:  is:nofirst  is:nolast
 *   is:nameless (legacy → both nofirst and nolast)
 *   first:/last:/phone:/nofirst/nolast also scope Messages “with person”
 *   within:  last-contact:  first-contact:  group-count:  message-count:
 *   from:  to:  with:  subject:  text:  has:attachment|noattachment
 *   filename:  filetype:  in:  after:  before:  source:
 *   is:group  is:direct
 *   "quoted phrases"  -term
 *
 * `from:me` = sent by vault owner. Other `from:` = sender match.
 * `to:` = addressed to (sent by me to them, or `to:me` = received).
 * `with:` = conversation involves this person (any participant).
 * `in:` = restrict to a conversation (title / handle); `in:trash` ignored.
 * `within:` / `label:` = contacts on one label.
 *
 * `after:` / `before:` accept YYYY, YYYY-MM-DD, or relative `7d` / `1w` / `1m` / `1y`.
 *
 * `first-contact:` / `last-contact:` bound a contact's overall first / last
 * message date (`>=D`, `<D`, `D1..D2`, bare `D` = before).
 */

/** Inclusive lower bound / upper bound, both `YYYY-MM-DD`. */
export type DateBounds = {
  from: string | null;
  to: string | null;
};

export type ParsedSearchQuery = {
  /** Queries without an explicit search operator retain legacy message mode. */
  mode: "messages" | "contacts";
  /** Free-text terms (AND) searched via FTS. */
  terms: string[];
  /** Phrases that must appear (AND). */
  phrases: string[];
  /** Terms/phrases to exclude. */
  exclude: string[];
  /** Sender filter; use `me` for vault owner. */
  from: string | null;
  /** Addressed-to filter; use `me` for received messages. */
  to: string | null;
  /** Conversation involves this person (sender or recipient). */
  with: string | null;
  subject: string | null;
  /** Explicit body-only FTS terms. */
  text: string | null;
  /** null = any, true = has attachment, false = no attachment. */
  hasAttachment: boolean | null;
  filename: string | null;
  /** image | video | audio | document | contact | other | pdf (→ document). */
  filetype: string | null;
  /** Restrict to a conversation by title or handle (`in:`). */
  inConversation: string | null;
  after: string | null;
  before: string | null;
  source: string | null;
  conversationType: "group" | "individual" | null;
  /** Label whose contacts to search, active or not. */
  within: string | null;
  /** Contact handle (name or phone number). Legacy combined filter. */
  handle: string | null;
  /** Substring match on contact first name. */
  firstName: string | null;
  /** Substring match on contact last name. */
  lastName: string | null;
  /** Substring match on phone / email handles only. */
  phone: string | null;
  /** Contacts with empty first name. */
  noFirstName: boolean;
  /** Contacts with empty last name. */
  noLastName: boolean;
  /** Bounds on each contact's overall last message date. */
  lastContact: DateBounds;
  /** Bounds on each contact's overall first message date. */
  firstContact: DateBounds;
  groupCount: CountComparison | null;
  messageCount: CountComparison | null;
  /** @deprecated Retained for untouched legacy consumers; always false. */
  showContact: boolean;
};

/** Which bound(s) a date field contributes. */
export type DateFilterMode = "any" | "on-or-after" | "before" | "between";

export type DateFilterInput = {
  mode: DateFilterMode;
  from?: string;
  to?: string;
};

export type CountComparator = "=" | ">" | ">=" | "<" | "<=";

export type CountComparison = {
  comparator: CountComparator;
  value: number;
};

export type CountFilterInput = {
  comparator: CountComparator | "any";
  value?: string;
};

export type AttachmentFilter = "any" | "yes" | "no";

export type AdvancedSearchForm = {
  mode?: "messages" | "contacts";
  /** Label to search within; empty means all contacts. */
  within?: string;
  /** Name or number of a conversation participant (`with:`). */
  withPerson?: string;
  /** Sender (`from:`); use `me` for vault owner. */
  fromPerson?: string;
  /** Addressed-to (`to:`); use `me` for received. */
  toPerson?: string;
  /** Contact handle (name or phone number). Legacy combined filter. */
  handle?: string;
  firstName?: string;
  lastName?: string;
  phone?: string;
  /** Contacts search: only contacts with empty first name. */
  noFirstName?: boolean;
  /** Contacts search: only contacts with empty last name. */
  noLastName?: boolean;
  hasWords?: string;
  doesntHave?: string;
  subject?: string;
  filename?: string;
  filetype?: string;
  /** Conversation title / handle (`in:`). */
  inConversation?: string;
  /** Message timestamp bounds. */
  date?: DateFilterInput;
  /** Contact's first message date bounds. */
  firstContact?: DateFilterInput;
  /** Contact's last message date bounds. */
  lastContact?: DateFilterInput;
  groupCount?: CountFilterInput;
  messageCount?: CountFilterInput;
  conversationType?: "any" | "group" | "individual";
  source?: string;
  /** @deprecated Prefer attachmentFilter; true maps to "yes". */
  hasAttachment?: boolean;
  attachmentFilter?: AttachmentFilter;
};

const NO_BOUNDS: DateBounds = { from: null, to: null };

const EMPTY: ParsedSearchQuery = {
  mode: "messages",
  terms: [],
  phrases: [],
  exclude: [],
  from: null,
  to: null,
  with: null,
  subject: null,
  text: null,
  hasAttachment: null,
  filename: null,
  filetype: null,
  inConversation: null,
  after: null,
  before: null,
  source: null,
  conversationType: null,
  within: null,
  handle: null,
  firstName: null,
  lastName: null,
  phone: null,
  noFirstName: false,
  noLastName: false,
  lastContact: NO_BOUNDS,
  firstContact: NO_BOUNDS,
  groupCount: null,
  messageCount: null,
  showContact: false,
};

const OPERATOR_RE =
  /^(search|with|from|to|subject|text|has|after|before|source|is|within|label|in|show|handle|filename|filetype|last-contact|first-contact|group-count|message-count|first|last|phone):(.*)$/i;

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

/** Absolute YYYY / YYYY-MM-DD, or relative Nd/Nw/Nm/Ny → local calendar YYYY-MM-DD. */
function normalizeDate(raw: string): string | null {
  const t = raw.trim();
  if (!t) return null;
  const rel = t.match(/^(\d+)([dwmy])$/i);
  if (rel) {
    const n = Number.parseInt(rel[1]!, 10);
    if (!Number.isFinite(n) || n < 0) return null;
    const unit = rel[2]!.toLowerCase();
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    if (unit === "d") d.setDate(d.getDate() - n);
    else if (unit === "w") d.setDate(d.getDate() - n * 7);
    else if (unit === "m") d.setMonth(d.getMonth() - n);
    else d.setFullYear(d.getFullYear() - n);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
  }
  if (/^\d{4}-\d{2}-\d{2}$/.test(t)) return t;
  if (/^\d{4}$/.test(t)) return `${t}-01-01`;
  return t || null;
}

/** Normalize filetype aliases (e.g. pdf → document). */
export function normalizeFiletype(raw: string): string | null {
  const v = raw.trim().toLowerCase();
  if (!v) return null;
  if (v === "pdf") return "document";
  if (
    v === "image" ||
    v === "video" ||
    v === "audio" ||
    v === "document" ||
    v === "contact" ||
    v === "other"
  ) {
    return v;
  }
  return v;
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

function parseCountComparison(raw: string): CountComparison | null {
  const match = raw.trim().match(/^(>=|<=|>|<|=)(\d+)$/);
  if (!match) return null;
  return {
    comparator: match[1] as CountComparator,
    value: Number.parseInt(match[2]!, 10),
  };
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
        case "search": {
          const mode = value.toLowerCase();
          if (mode === "contacts" || mode === "messages") out.mode = mode;
          break;
        }
        case "from":
          out.from = value;
          break;
        case "to":
          out.to = value;
          break;
        case "with":
          out.with = value;
          break;
        case "subject":
          out.subject = value;
          break;
        case "text":
          out.text = value;
          break;
        case "has": {
          const v = value.toLowerCase();
          if (v === "attachment" || v === "att") out.hasAttachment = true;
          else if (v === "noattachment" || v === "noatt") {
            out.hasAttachment = false;
          }
          break;
        }
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
          } else if (v === "nofirst") {
            out.noFirstName = true;
          } else if (v === "nolast") {
            out.noLastName = true;
          } else if (v === "nameless") {
            // Legacy: both empty.
            out.noFirstName = true;
            out.noLastName = true;
          }
          break;
        }
        case "within":
        case "label":
          out.within = value;
          break;
        case "handle":
          out.handle = value;
          break;
        case "first":
          out.firstName = value;
          break;
        case "last":
          out.lastName = value;
          break;
        case "phone":
          out.phone = value;
          break;
        case "filename":
          out.filename = value;
          break;
        case "filetype":
          out.filetype = normalizeFiletype(value);
          break;
        case "in":
          // Legacy in:trash — trash is always excluded now.
          if (value.toLowerCase() !== "trash") out.inConversation = value;
          break;
        case "show":
          // Retired legacy presentation operator.
          break;
        case "last-contact":
          out.lastContact = parseDateBounds(value);
          break;
        case "first-contact":
          out.firstContact = parseDateBounds(value);
          break;
        case "group-count":
          out.groupCount = parseCountComparison(value);
          break;
        case "message-count":
          out.messageCount = parseCountComparison(value);
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

function quoteToken(value: string): string {
  return /\s/.test(value) ? `"${value.replace(/"/g, "")}"` : value;
}

function composeCountComparison(input: CountFilterInput | undefined): string | null {
  if (!input || input.comparator === "any") return null;
  const value = input.value?.trim() ?? "";
  if (!/^\d+$/.test(value)) return null;
  return `${input.comparator}${value}`;
}

/** Inverse of composeDateBounds / after+before pairing. */
function dateFilterFromBounds(from: string | null, to: string | null): DateFilterInput {
  if (from && to) return { mode: "between", from, to };
  if (from) return { mode: "on-or-after", from, to: "" };
  if (to) return { mode: "before", from: "", to };
  return { mode: "any", from: "", to: "" };
}

/**
 * Reverse-parse a vault search string into advanced-form fields so reopening
 * the dropdown can restore fields from the query bar.
 */
export function formFromSearchQuery(query: string): AdvancedSearchForm {
  const parsed = parseSearchQuery(query);
  const hasWords = [
    ...parsed.terms,
    ...parsed.phrases.map((p) => quoteToken(p)),
  ].join(" ");
  const doesntHave = parsed.exclude.map((e) => quoteToken(e)).join(" ");
  const attachmentFilter: AttachmentFilter =
    parsed.hasAttachment === true
      ? "yes"
      : parsed.hasAttachment === false
        ? "no"
        : "any";

  return {
    mode: parsed.mode,
    within: parsed.within ?? undefined,
    // Combined Handle stays on handle:; split fields stay on first/last/phone.
    handle: parsed.handle ?? undefined,
    firstName: parsed.firstName ?? undefined,
    lastName: parsed.lastName ?? undefined,
    phone: parsed.phone ?? undefined,
    noFirstName: parsed.noFirstName || undefined,
    noLastName: parsed.noLastName || undefined,
    withPerson: parsed.with ?? undefined,
    fromPerson: parsed.from ?? undefined,
    toPerson: parsed.to ?? undefined,
    hasWords: hasWords || undefined,
    doesntHave: doesntHave || undefined,
    subject: parsed.subject ?? undefined,
    filename: parsed.filename ?? undefined,
    filetype: parsed.filetype ?? undefined,
    inConversation: parsed.inConversation ?? undefined,
    date: dateFilterFromBounds(parsed.after, parsed.before),
    firstContact: dateFilterFromBounds(
      parsed.firstContact.from,
      parsed.firstContact.to,
    ),
    lastContact: dateFilterFromBounds(
      parsed.lastContact.from,
      parsed.lastContact.to,
    ),
    groupCount: parsed.groupCount
      ? {
          comparator: parsed.groupCount.comparator,
          value: String(parsed.groupCount.value),
        }
      : { comparator: "any", value: "" },
    messageCount: parsed.messageCount
      ? {
          comparator: parsed.messageCount.comparator,
          value: String(parsed.messageCount.value),
        }
      : { comparator: "any", value: "" },
    conversationType: parsed.conversationType ?? "any",
    source: parsed.source ?? undefined,
    attachmentFilter,
    hasAttachment: parsed.hasAttachment === true,
  };
}

/** Build a shareable query string from the advanced form fields. */
export function composeSearchQuery(form: AdvancedSearchForm): string {
  const parts: string[] = [];
  const quoteIfNeeded = (v: string) =>
    /\s/.test(v) ? `"${v.replace(/"/g, "")}"` : v;

  if (form.mode === "contacts") parts.push("search:contacts");
  if (form.within?.trim()) {
    parts.push(`within:${quoteIfNeeded(form.within.trim())}`);
  }
  // first/last/phone apply in Contacts search and Messages “with person” expand.
  if (form.noFirstName) parts.push("is:nofirst");
  else if (form.firstName?.trim()) {
    parts.push(`first:${quoteIfNeeded(form.firstName.trim())}`);
  }
  if (form.noLastName) parts.push("is:nolast");
  else if (form.lastName?.trim()) {
    parts.push(`last:${quoteIfNeeded(form.lastName.trim())}`);
  }
  if (form.phone?.trim()) {
    parts.push(`phone:${quoteIfNeeded(form.phone.trim())}`);
  }
  if (form.mode === "contacts" && form.handle?.trim()) {
    parts.push(`handle:${quoteIfNeeded(form.handle.trim())}`);
  }
  if (form.mode !== "contacts" && form.fromPerson?.trim()) {
    parts.push(`from:${quoteIfNeeded(form.fromPerson.trim())}`);
  }
  if (form.mode !== "contacts" && form.toPerson?.trim()) {
    parts.push(`to:${quoteIfNeeded(form.toPerson.trim())}`);
  }
  if (form.mode !== "contacts" && form.withPerson?.trim()) {
    parts.push(`with:${quoteIfNeeded(form.withPerson.trim())}`);
  }
  if (form.mode !== "contacts" && form.inConversation?.trim()) {
    parts.push(`in:${quoteIfNeeded(form.inConversation.trim())}`);
  }
  if (form.mode !== "contacts" && form.subject?.trim()) {
    parts.push(`subject:${quoteIfNeeded(form.subject.trim())}`);
  }
  if (form.mode !== "contacts" && form.hasWords?.trim()) {
    parts.push(form.hasWords.trim());
  }
  if (form.mode !== "contacts" && form.doesntHave?.trim()) {
    for (const t of tokenize(form.doesntHave)) {
      if (t.startsWith('"')) parts.push(`-${t}`);
      else parts.push(`-${t.replace(/^-/, "")}`);
    }
  }

  const date = form.mode !== "contacts" ? form.date : undefined;
  if (date && date.mode !== "any") {
    const from = date.from?.trim();
    const to = date.to?.trim();
    if (date.mode !== "before" && from) parts.push(`after:${from}`);
    if (date.mode !== "on-or-after" && to) parts.push(`before:${to}`);
  }

  const firstContact =
    form.mode === "contacts" ? composeDateBounds(form.firstContact) : null;
  if (firstContact) parts.push(`first-contact:${firstContact}`);
  const lastContact =
    form.mode === "contacts" ? composeDateBounds(form.lastContact) : null;
  if (lastContact) parts.push(`last-contact:${lastContact}`);

  const groupCount =
    form.mode === "contacts" ? composeCountComparison(form.groupCount) : null;
  if (groupCount) parts.push(`group-count:${groupCount}`);
  const messageCount =
    form.mode === "contacts" ? composeCountComparison(form.messageCount) : null;
  if (messageCount) parts.push(`message-count:${messageCount}`);

  if (form.mode !== "contacts") {
    if (form.conversationType === "group") parts.push("is:group");
    if (form.conversationType === "individual") parts.push("is:direct");
    if (form.source?.trim()) parts.push(`source:${form.source.trim()}`);
    const attachment =
      form.attachmentFilter ??
      (form.hasAttachment ? "yes" : "any");
    if (attachment === "yes") parts.push("has:attachment");
    if (attachment === "no") parts.push("has:noattachment");
    if (form.filename?.trim()) {
      parts.push(`filename:${quoteIfNeeded(form.filename.trim())}`);
    }
    if (form.filetype?.trim() && form.filetype !== "any") {
      parts.push(`filetype:${quoteIfNeeded(form.filetype.trim())}`);
    }
  }
  return parts.join(" ");
}

export function hasSearchCriteria(q: ParsedSearchQuery): boolean {
  return (
    q.terms.length > 0 ||
    q.phrases.length > 0 ||
    q.exclude.length > 0 ||
    q.mode === "contacts" ||
    !!q.from ||
    !!q.to ||
    !!q.with ||
    !!q.subject ||
    !!q.text ||
    q.hasAttachment !== null ||
    !!q.filename ||
    !!q.filetype ||
    !!q.inConversation ||
    !!q.after ||
    !!q.before ||
    !!q.source ||
    !!q.conversationType ||
    !!q.within ||
    !!q.handle ||
    !!q.firstName ||
    !!q.lastName ||
    !!q.phone ||
    q.noFirstName ||
    q.noLastName ||
    !!q.groupCount ||
    !!q.messageCount ||
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
  if (q.text?.trim()) {
    const safe = q.text.trim().replace(/"/g, "");
    if (safe) parts.push(`body:"${safe}"`);
  }
  if (parts.length === 0) return null;
  return parts.join(" AND ");
}
