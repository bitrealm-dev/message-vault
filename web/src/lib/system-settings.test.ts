import { describe, expect, it } from "vitest";
import {
  defaultImportStagingDir,
  joinImportStagingPath,
  resolveImportStagingDir,
} from "./system-settings";

describe("defaultImportStagingDir", () => {
  it("joins message-vault under the home folder", () => {
    expect(defaultImportStagingDir("/home/mbeisser")).toBe("/home/mbeisser/message-vault");
  });

  it("strips a trailing slash on the home folder", () => {
    expect(defaultImportStagingDir("/home/mbeisser/")).toBe("/home/mbeisser/message-vault");
  });

  it("uses a relative message-vault path when home is empty", () => {
    expect(defaultImportStagingDir("")).toBe("message-vault");
  });
});

describe("joinImportStagingPath", () => {
  const now = new Date(2026, 7, 24, 18, 5, 9);

  it("puts the staging folder name directly under the parent", () => {
    expect(joinImportStagingPath("/home/mbeisser/message-vault", "imessage-ios", now)).toBe(
      "/home/mbeisser/message-vault/staging-iphone-ios-260824-180509",
    );
  });

  it("does not nest another message-vault under a custom parent", () => {
    expect(joinImportStagingPath("/data/imports", "imessage-ios", now)).toBe(
      "/data/imports/staging-iphone-ios-260824-180509",
    );
  });

  it("keeps SMS Backup & Restore source ids in the folder name", () => {
    expect(joinImportStagingPath("/Users/sam/message-vault", "sms-backup-restore", now)).toBe(
      "/Users/sam/message-vault/staging-sms-backup-restore-260824-180509",
    );
  });

  it("strips a trailing slash on the parent folder", () => {
    expect(joinImportStagingPath("/home/mbeisser/message-vault/", "imessage-macos", now)).toBe(
      "/home/mbeisser/message-vault/staging-macos-260824-180509",
    );
  });

  it("strips a trailing backslash on a Windows parent folder", () => {
    expect(joinImportStagingPath("C:\\Users\\sam\\message-vault\\", "imessage-ios", now)).toBe(
      "C:\\Users\\sam\\message-vault/staging-iphone-ios-260824-180509",
    );
  });

  it("uses only the staging folder name when the parent is empty", () => {
    expect(joinImportStagingPath("", "imessage-ios", now)).toBe("staging-iphone-ios-260824-180509");
  });
});

describe("resolveImportStagingDir", () => {
  it("fails when the user home directory cannot be determined and no parent is saved", async () => {
    await expect(resolveImportStagingDir("/backup", "imessage-ios")).rejects.toThrow(
      /home directory/i,
    );
  });
});
