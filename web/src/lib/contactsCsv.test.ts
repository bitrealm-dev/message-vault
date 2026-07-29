import assert from "node:assert/strict";
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
        firstName: "Ada",
        lastName: "Lovelace",
        exclude: false,
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
        firstName: "Ada",
        lastName: null,
        exclude: true,
        labels,
      },
    ]);
    const lines = csv.trimEnd().split("\n");
    const header = parseCsvLine(lines[0]!);
    assert.deepEqual(header, contactsCsvHeader(7));
    const row = parseCsvLine(lines[1]!);
    assert.equal(row[0], "+15551234567");
    assert.equal(row[1], "Ada");
    assert.equal(row[3], "true");
    assert.deepEqual(row.slice(4), labels);
  });
});
