import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import {
  contactsCsvHeader,
  parseCsvLine,
  serializeContactsCsv,
} from "./contactsCsv";

describe("serializeContactsCsv", () => {
  it("writes at least five label columns", () => {
    const csv = serializeContactsCsv([
      {
        phones: ["+15551234567"],
        preferredName: "Ada Lovelace",
        labels: ["Family"],
      },
    ]);
    const header = parseCsvLine(csv.split("\n")[0]!);
    assert.deepEqual(header, contactsCsvHeader(5));
    assert.ok(header.includes("label_5"));
  });

  it("expands beyond five labels", () => {
    const labels = ["A", "B", "C", "D", "E", "F", "G"];
    const csv = serializeContactsCsv([
      {
        phones: ["+15551234567"],
        preferredName: "Mononym",
        labels,
      },
    ]);
    const lines = csv.trimEnd().split("\n");
    const header = parseCsvLine(lines[0]!);
    assert.deepEqual(header, contactsCsvHeader(7));
    const row = parseCsvLine(lines[1]!);
    assert.equal(row[0], "+15551234567");
    assert.equal(row[1], "Mononym");
    assert.equal(row[2], "");
    assert.deepEqual(row.slice(3), labels);
    const fixture = fs.readFileSync(
      path.join(process.cwd(), "..", "fixtures", "contacts", "current-labels.csv"),
      "utf8",
    );
    assert.equal(csv, fixture);
  });
});
