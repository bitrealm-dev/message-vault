/**
 * Worker for scripts/deprecated/regen-search-goldens.mjs — run via tsx only.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseSearchQuery } from "../../web-next/src/lib/searchQuery.ts";

type Case = {
  name: string;
  input: string;
  expected?: unknown;
};

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const casesPath = path.join(root, "tests", "fixtures", "search", "parse-cases.json");

const cases = JSON.parse(fs.readFileSync(casesPath, "utf8")) as Case[];
for (const c of cases) {
  c.expected = parseSearchQuery(c.input);
}

fs.writeFileSync(casesPath, `${JSON.stringify(cases, null, 2)}\n`, "utf8");
console.log(`regen-search-goldens: wrote ${cases.length} cases → ${path.relative(root, casesPath)}`);
