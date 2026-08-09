import { phoneHandlesOnly } from "./handleKind";

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

function splitPreferredName(
  preferredName: string | null,
): [firstName: string, lastName: string] {
  const value = (preferredName ?? "").trim();
  if (!value) return ["", ""];
  const space = value.indexOf(" ");
  if (space < 0) return [value, ""];
  return [value.slice(0, space).trim(), value.slice(space + 1).trim()];
}

/** Build an explicit contacts CSV export header. */
export function contactsCsvHeader(labelCount: number): string[] {
  const n = Math.max(5, labelCount);
  const header = ["phones", "first_name", "last_name"];
  for (let i = 1; i <= n; i++) header.push(`label_${i}`);
  return header;
}

export type VaultContactCsvRow = {
  phones: string[];
  preferredName: string | null;
  labels: string[];
};

/** Serialize contact rows for an explicit download; this never writes a file. */
export function serializeContactsCsv(rows: VaultContactCsvRow[]): string {
  const maxLabels = rows.reduce((m, r) => Math.max(m, r.labels.length), 0);
  const header = contactsCsvHeader(maxLabels);
  const lines = [header.map(escapeCsvField).join(",")];
  for (const row of rows) {
    const [firstName, lastName] = splitPreferredName(row.preferredName);
    const cols = header.map(() => "");
    cols[0] = phoneHandlesOnly(row.phones).join(";");
    cols[1] = firstName;
    cols[2] = lastName;
    for (let i = 0; i < Math.max(5, maxLabels); i++) {
      cols[3 + i] = row.labels[i] ?? "";
    }
    lines.push(cols.map(escapeCsvField).join(","));
  }
  return `${lines.join("\n")}\n`;
}
