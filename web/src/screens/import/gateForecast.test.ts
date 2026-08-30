import { describe, expect, it } from "vitest";
import { forecastGroups, pluralFiles, verdictCopy } from "./gateForecast";

describe("pluralFiles", () => {
  it("keeps the noun singular for exactly one file", () => {
    expect(pluralFiles(1)).toBe("1 file");
  });

  it("pluralizes for zero and for more than one", () => {
    expect(pluralFiles(0)).toBe("0 files");
    expect(pluralFiles(2)).toBe("2 files");
    expect(pluralFiles(1234)).toBe("1,234 files");
  });
});

describe("verdictCopy", () => {
  it("says which job the estimate is about", () => {
    expect(verdictCopy("likely_fits", "convert").label).toBe("Likely to fit after converting");
    expect(verdictCopy("likely_fits", "compress").label).toBe("Likely to fit after compressing");
  });

  it("warns that a file which fits today may not afterwards", () => {
    // Decision 12's whole point: conversion is not a size reduction, and an
    // iPhone backup is mostly formats that grow.
    expect(verdictCopy("may_grow", "convert").label).toBe("May grow past the limit");
  });

  it("names the two settled states plainly", () => {
    expect(verdictCopy("fits_as_is", "convert").label).toBe("Fits as-is");
    expect(verdictCopy("probably_too_big", "convert").label).toBe("Probably still too big");
  });

  it("explains a file the media step cannot touch", () => {
    expect(verdictCopy("cannot_process", "convert").label).toBe(
      "Cannot be converted — not audio or video",
    );
  });

  it("never says transcode", () => {
    const modes = ["convert", "compress"] as const;
    const verdicts = [
      "fits_as_is",
      "likely_fits",
      "may_grow",
      "probably_too_big",
      "cannot_process",
    ] as const;
    for (const mode of modes) {
      for (const verdict of verdicts) {
        expect(verdictCopy(verdict, mode).label.toLowerCase()).not.toContain("transcode");
      }
    }
  });
});

describe("forecastGroups", () => {
  it("drops the states with nothing in them", () => {
    // A row reading "0 files may grow past the limit" is noise on a screen
    // whose job is to be read quickly. The remaining two groups keep the
    // priority order from decision 11: may_grow needs attention, fits_as_is
    // does not, so may_grow comes first.
    const groups = forecastGroups(
      { fitsAsIs: 12, likelyFits: 0, mayGrow: 2, probablyTooBig: 0, cannotProcess: 0 },
      "convert",
    );
    expect(groups.map((g) => g.verdict)).toEqual(["may_grow", "fits_as_is"]);
  });

  it("puts the states that need attention first", () => {
    const groups = forecastGroups(
      { fitsAsIs: 12, likelyFits: 3, mayGrow: 2, probablyTooBig: 1, cannotProcess: 1 },
      "convert",
    );
    expect(groups[0].verdict).toBe("probably_too_big");
    expect(groups.at(-1)?.verdict).toBe("fits_as_is");
  });

  it("carries the count and copy for each surviving group", () => {
    const groups = forecastGroups(
      { fitsAsIs: 0, likelyFits: 0, mayGrow: 0, probablyTooBig: 4, cannotProcess: 0 },
      "compress",
    );
    expect(groups).toEqual([
      {
        verdict: "probably_too_big",
        count: 4,
        label: "Probably still too big",
        hint: "Expected to stay over the limit even after the media step.",
      },
    ]);
  });
});
