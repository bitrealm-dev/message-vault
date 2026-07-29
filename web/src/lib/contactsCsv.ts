import fs from "fs";
import path from "path";
import { currentAccountId } from "./accountScope";
import { phoneHandlesOnly } from "./handleKind";
import { accountDataDir, repoRoot } from "./paths";

const DEFAULT_CONTACTS_CSV_HEADER =
  "phones,first_name,last_name,exclude,label_1,label_2,label_3,label_4,label_5\n";

/** Per-account contacts CSV under `data/<account_id>/contacts.csv`. */
function contactsCsvPath(accountId = currentAccountId()): string {
  const dest = path.join(accountDataDir(accountId), "contacts.csv");
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    const legacy = legacyContactsCsvPath();
    if (legacy && fs.existsSync(legacy)) {
      fs.copyFileSync(legacy, dest);
    } else {
      fs.writeFileSync(dest, DEFAULT_CONTACTS_CSV_HEADER, "utf8");
    }
  }
  return dest;
}

/** Optional seed/template contacts.csv (demo bundle or repo template). */
function legacyContactsCsvPath(): string | null {
  const candidates = [
    path.join(repoRoot(), "demo", "config", "contacts.csv"),
    path.join(repoRoot(), "config", "contacts.csv"),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

export function parseCsvLine(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]!;
    if (inQuotes) {
      if (ch === '"' && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (ch === '"') {
        inQuotes = false;
      } else {
        cur += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ",") {
      out.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  out.push(cur);
  return out;
}

export function escapeCsvField(value: string): string {
  if (/[",\n\r]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

function parseNumberedColumn(header: string, prefix: string): number | null {
  if (!header.startsWith(prefix)) return null;
  const rest = header.slice(prefix.length);
  if (!rest || !/^\d+$/.test(rest)) return null;
  const n = Number(rest);
  return n >= 1 ? n : null;
}

/** Ordered label slots: prefer label_N over legacy group_N for each index. */
function labelColumnIndexes(header: string[]): number[] {
  let maxN = 0;
  for (const h of header) {
    const labelN = parseNumberedColumn(h, "label_");
    const groupN = parseNumberedColumn(h, "group_");
    if (labelN != null) maxN = Math.max(maxN, labelN);
    if (groupN != null) maxN = Math.max(maxN, groupN);
  }
  if (maxN === 0) return [];

  const indexes: number[] = [];
  for (let n = 1; n <= maxN; n++) {
    const labelI = header.indexOf(`label_${n}`);
    const groupI = header.indexOf(`group_${n}`);
    if (labelI >= 0) indexes.push(labelI);
    else if (groupI >= 0) indexes.push(groupI);
  }
  return indexes;
}

/** Read labels from label_* and/or group_* columns (label wins per slot). */
function readCsvLabels(cols: string[], header: string[]): string[] {
  let maxN = 0;
  for (const h of header) {
    const labelN = parseNumberedColumn(h, "label_");
    const groupN = parseNumberedColumn(h, "group_");
    if (labelN != null) maxN = Math.max(maxN, labelN);
    if (groupN != null) maxN = Math.max(maxN, groupN);
  }

  const seen = new Set<string>();
  const out: string[] = [];
  for (let n = 1; n <= maxN; n++) {
    const labelCol = header.indexOf(`label_${n}`);
    const groupCol = header.indexOf(`group_${n}`);
    const raw =
      (labelCol >= 0 ? (cols[labelCol] ?? "").trim() : "") ||
      (groupCol >= 0 ? (cols[groupCol] ?? "").trim() : "");
    if (!raw) continue;
    const key = raw.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(raw);
  }
  return out;
}

function writeCsvLabels(
  cols: string[],
  labelIdx: number[],
  labels: string[],
): void {
  const unique: string[] = [];
  const seen = new Set<string>();
  for (const label of labels) {
    const trimmed = label.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(trimmed);
  }
  for (let i = 0; i < labelIdx.length; i++) {
    const col = labelIdx[i]!;
    if (col < 0) continue;
    cols[col] = unique[i] ?? "";
  }
}

function requireLabelColumns(header: string[]): number[] {
  const labelIdx = labelColumnIndexes(header);
  if (labelIdx.length === 0) {
    throw new Error(
      "contacts CSV missing label_N (or legacy group_N) columns",
    );
  }
  return labelIdx;
}

/**
 * Ensure the CSV header has at least `needed` label_N columns.
 * Expands existing rows with empty cells when columns are added.
 */
function ensureLabelColumnCapacity(
  lines: string[],
  header: string[],
  needed: number,
): { header: string[]; labelIdx: number[]; lines: string[] } {
  let maxN = 0;
  for (const h of header) {
    const labelN = parseNumberedColumn(h, "label_");
    const groupN = parseNumberedColumn(h, "group_");
    if (labelN != null) maxN = Math.max(maxN, labelN);
    if (groupN != null) maxN = Math.max(maxN, groupN);
  }
  const target = Math.max(needed, maxN, 5);
  if (target <= maxN && labelColumnIndexes(header).length > 0) {
    return { header, labelIdx: requireLabelColumns(header), lines };
  }

  const nextHeader = [...header];
  for (let n = 1; n <= target; n++) {
    const name = `label_${n}`;
    if (!nextHeader.includes(name) && !nextHeader.includes(`group_${n}`)) {
      nextHeader.push(name);
    } else if (!nextHeader.includes(name) && nextHeader.includes(`group_${n}`)) {
      // Keep legacy group_N; labelColumnIndexes prefers label when both exist.
    }
  }

  const nextLines = lines.map((line, lineNo) => {
    if (lineNo === 0) return nextHeader.join(",");
    if (!line.trim()) return line;
    const cols = parseCsvLine(line);
    while (cols.length < nextHeader.length) cols.push("");
    return cols.map(escapeCsvField).join(",");
  });
  if (nextLines.length === 0) {
    nextLines.push(nextHeader.join(","));
  } else {
    nextLines[0] = nextHeader.join(",");
  }

  return {
    header: nextHeader,
    labelIdx: requireLabelColumns(nextHeader),
    lines: nextLines,
  };
}

export function updateContactsCsv(
  matchPhones: string[],
  matchNames: { firstName: string | null; lastName: string | null },
  patch: {
    exclude: boolean;
    groups: string[];
    firstName?: string | null;
    lastName?: string | null;
    phones?: string[];
  },
): void {
  const csvPath = contactsCsvPath();
  if (!fs.existsSync(csvPath)) {
    throw new Error(`contacts CSV not found: ${csvPath}`);
  }

  const phoneSet = new Set(matchPhones);
  const raw = fs.readFileSync(csvPath, "utf8");
  let lines = raw.split(/\r?\n/);
  if (lines.length === 0) {
    throw new Error("contacts CSV is empty");
  }

  let header = parseCsvLine(lines[0] ?? "");
  const expanded = ensureLabelColumnCapacity(lines, header, patch.groups.length);
  header = expanded.header;
  lines = expanded.lines;
  const labelIdx = expanded.labelIdx;

  const idx = {
    phones: header.indexOf("phones"),
    firstName: header.indexOf("first_name"),
    lastName: header.indexOf("last_name"),
    exclude: header.indexOf("exclude"),
  };
  if (idx.phones < 0 || idx.exclude < 0) {
    throw new Error("contacts CSV missing required columns");
  }

  const matchFirst = (matchNames.firstName ?? "").trim().toLowerCase();
  const matchLast = (matchNames.lastName ?? "").trim().toLowerCase();

  let matched = false;
  const out = lines.map((line, lineNo) => {
    if (lineNo === 0 || !line.trim()) return line;
    const cols = parseCsvLine(line);
    while (cols.length < header.length) cols.push("");
    const rowPhones = (cols[idx.phones] ?? "")
      .split(";")
      .map((p) => p.trim())
      .filter(Boolean);
    const phoneHit =
      phoneSet.size > 0 && rowPhones.some((p) => phoneSet.has(p));
    const nameHit =
      !phoneHit &&
      phoneSet.size === 0 &&
      idx.firstName >= 0 &&
      idx.lastName >= 0 &&
      (cols[idx.firstName] ?? "").trim().toLowerCase() === matchFirst &&
      (cols[idx.lastName] ?? "").trim().toLowerCase() === matchLast &&
      (matchFirst !== "" || matchLast !== "");
    if (!phoneHit && !nameHit) {
      return line;
    }
    matched = true;
    if (patch.phones) {
      cols[idx.phones] = phoneHandlesOnly(patch.phones).join(";");
    }
    if (patch.firstName !== undefined && idx.firstName >= 0) {
      cols[idx.firstName] = patch.firstName ?? "";
    }
    if (patch.lastName !== undefined && idx.lastName >= 0) {
      cols[idx.lastName] = patch.lastName ?? "";
    }
    cols[idx.exclude] = patch.exclude ? "true" : "false";
    writeCsvLabels(cols, labelIdx, patch.groups);
    return cols.map(escapeCsvField).join(",");
  });

  if (!matched) {
    throw new Error("contact not found in contacts.csv");
  }

  const endsWithNewline = /\r?\n$/.test(raw);
  let body = out.join("\n");
  if (endsWithNewline && !body.endsWith("\n")) body += "\n";
  fs.writeFileSync(csvPath, body, "utf8");
}

export function appendContactsCsv(row: {
  phones: string[];
  firstName: string | null;
  lastName: string | null;
  exclude: boolean;
  groups: string[];
}): void {
  const csvPath = contactsCsvPath();
  if (!fs.existsSync(csvPath)) {
    throw new Error(`contacts CSV not found: ${csvPath}`);
  }

  const raw = fs.readFileSync(csvPath, "utf8");
  let lines = raw.split(/\r?\n/);
  if (lines.length === 0) {
    throw new Error("contacts CSV is empty");
  }

  let header = parseCsvLine(lines[0] ?? "");
  const expanded = ensureLabelColumnCapacity(lines, header, row.groups.length);
  header = expanded.header;
  lines = expanded.lines;
  const labelIdx = expanded.labelIdx;

  const idx = {
    phones: header.indexOf("phones"),
    firstName: header.indexOf("first_name"),
    lastName: header.indexOf("last_name"),
    exclude: header.indexOf("exclude"),
  };
  if (idx.phones < 0 || idx.exclude < 0) {
    throw new Error("contacts CSV missing required columns");
  }

  const cols = header.map(() => "");
  cols[idx.phones] = phoneHandlesOnly(row.phones).join(";");
  if (idx.firstName >= 0) cols[idx.firstName] = row.firstName ?? "";
  if (idx.lastName >= 0) cols[idx.lastName] = row.lastName ?? "";
  cols[idx.exclude] = row.exclude ? "true" : "false";
  writeCsvLabels(cols, labelIdx, row.groups);

  const line = cols.map(escapeCsvField).join(",");
  // Drop trailing empty lines so we append after the last data row.
  while (lines.length > 1 && !(lines[lines.length - 1] ?? "").trim()) {
    lines.pop();
  }
  lines.push(line);
  fs.writeFileSync(csvPath, `${lines.join("\n")}\n`, "utf8");
}


/** Rewrite label_N (or legacy group_*) in contacts.csv by mapping names. */
export function rewriteCsvLabels(
  mapLabel: (label: string) => string | null,
): void {
  const csvPath = contactsCsvPath();
  if (!fs.existsSync(csvPath)) {
    throw new Error(`contacts CSV not found: ${csvPath}`);
  }

  const raw = fs.readFileSync(csvPath, "utf8");
  const lines = raw.split(/\r?\n/);
  if (lines.length === 0) {
    throw new Error("contacts CSV is empty");
  }

  const header = parseCsvLine(lines[0] ?? "");
  const labelIdx = requireLabelColumns(header);

  const out = lines.map((line, lineNo) => {
    if (lineNo === 0 || !line.trim()) return line;
    const cols = parseCsvLine(line);
    while (cols.length < header.length) cols.push("");
    const labels = readCsvLabels(cols, header)
      .map(mapLabel)
      .filter((g): g is string => Boolean(g));
    writeCsvLabels(cols, labelIdx, labels);
    return cols.map(escapeCsvField).join(",");
  });

  const endsWithNewline = /\r?\n$/.test(raw);
  let body = out.join("\n");
  if (endsWithNewline && !body.endsWith("\n")) body += "\n";
  fs.writeFileSync(csvPath, body, "utf8");
}


export function removeContactsCsv(
  targets: Array<{
    phones: string[];
    firstName: string | null;
    lastName: string | null;
  }>,
): void {
  if (targets.length === 0) return;

  const csvPath = contactsCsvPath();
  if (!fs.existsSync(csvPath)) {
    throw new Error(`contacts CSV not found: ${csvPath}`);
  }

  const raw = fs.readFileSync(csvPath, "utf8");
  const lines = raw.split(/\r?\n/);
  if (lines.length === 0) {
    throw new Error("contacts CSV is empty");
  }

  const header = parseCsvLine(lines[0] ?? "");
  const idx = {
    phones: header.indexOf("phones"),
    firstName: header.indexOf("first_name"),
    lastName: header.indexOf("last_name"),
  };
  if (idx.phones < 0) {
    throw new Error("contacts CSV missing required columns");
  }

  const matchers = targets.map((t) => ({
    phones: new Set(phoneHandlesOnly(t.phones)),
    first: (t.firstName ?? "").trim().toLowerCase(),
    last: (t.lastName ?? "").trim().toLowerCase(),
  }));

  const out = lines.filter((line, lineNo) => {
    if (lineNo === 0 || !line.trim()) return true;
    const cols = parseCsvLine(line);
    const rowPhones = (cols[idx.phones] ?? "")
      .split(";")
      .map((p) => p.trim())
      .filter(Boolean);
    const rowFirst =
      idx.firstName >= 0
        ? (cols[idx.firstName] ?? "").trim().toLowerCase()
        : "";
    const rowLast =
      idx.lastName >= 0 ? (cols[idx.lastName] ?? "").trim().toLowerCase() : "";

    for (const m of matchers) {
      const phoneHit =
        m.phones.size > 0 && rowPhones.some((p) => m.phones.has(p));
      const nameHit =
        !phoneHit &&
        m.phones.size === 0 &&
        (m.first !== "" || m.last !== "") &&
        rowFirst === m.first &&
        rowLast === m.last;
      if (phoneHit || nameHit) return false;
    }
    return true;
  });

  const endsWithNewline = /\r?\n$/.test(raw);
  let body = out.join("\n");
  if (endsWithNewline && !body.endsWith("\n")) body += "\n";
  fs.writeFileSync(csvPath, body, "utf8");
}

/** Build contacts CSV header with at least 5 label columns (more when needed). */
export function contactsCsvHeader(labelCount: number): string[] {
  const n = Math.max(5, labelCount);
  const header = ["phones", "first_name", "last_name", "exclude"];
  for (let i = 1; i <= n; i++) header.push(`label_${i}`);
  return header;
}

export type VaultContactCsvRow = {
  phones: string[];
  firstName: string | null;
  lastName: string | null;
  exclude: boolean;
  labels: string[];
};

/** Serialize vault contact rows to CSV text (dynamic label_N columns). */
export function serializeContactsCsv(rows: VaultContactCsvRow[]): string {
  const maxLabels = rows.reduce((m, r) => Math.max(m, r.labels.length), 0);
  const header = contactsCsvHeader(maxLabels);
  const lines = [header.map(escapeCsvField).join(",")];
  for (const row of rows) {
    const cols = header.map(() => "");
    cols[0] = phoneHandlesOnly(row.phones).join(";");
    cols[1] = row.firstName ?? "";
    cols[2] = row.lastName ?? "";
    cols[3] = row.exclude ? "true" : "false";
    for (let i = 0; i < Math.max(5, maxLabels); i++) {
      cols[4 + i] = row.labels[i] ?? "";
    }
    lines.push(cols.map(escapeCsvField).join(","));
  }
  return `${lines.join("\n")}\n`;
}
