import { describe, expect, it } from "vitest";
import { buildSourceFingerprint } from "./importSession";

describe("buildSourceFingerprint", () => {
  it("records the path, size, and mtime of the backup", () => {
    expect(
      buildSourceFingerprint("/Users/u/Backup/abc", {
        exists: true,
        isFile: false,
        isDirectory: true,
        sizeBytes: 4096,
        modifiedUnixMs: 1_756_512_000_000,
      }),
    ).toEqual({
      path: "/Users/u/Backup/abc",
      size_bytes: 4096,
      modified_unix_ms: 1_756_512_000_000,
      message_count: null,
    });
  });

  it("leaves the message count null until parse has run", () => {
    const fp = buildSourceFingerprint("/b", {
      exists: true,
      isFile: true,
      isDirectory: false,
      sizeBytes: 1,
      modifiedUnixMs: null,
    });
    expect(fp.message_count).toBeNull();
  });
});
