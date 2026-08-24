import { describe, expect, it } from "vitest";
import { joinImportStagingPath } from "./system-settings";

describe("joinImportStagingPath", () => {
  const now = new Date(2026, 7, 24, 18, 5, 9);

  it("puts the existing staging folder name under home/message-vault", () => {
    expect(joinImportStagingPath("/home/mbeisser", "imessage-ios", now)).toBe(
      "/home/mbeisser/message-vault/staging-iphone-ios-260824-180509",
    );
  });

  it("keeps SMS Backup & Restore source ids in the folder name", () => {
    expect(joinImportStagingPath("/Users/sam", "sms-backup-restore", now)).toBe(
      "/Users/sam/message-vault/staging-sms-backup-restore-260824-180509",
    );
  });

  it("strips a trailing slash on the home folder", () => {
    expect(joinImportStagingPath("/home/mbeisser/", "imessage-macos", now)).toBe(
      "/home/mbeisser/message-vault/staging-macos-260824-180509",
    );
  });

  it("uses a relative message-vault path when home is empty", () => {
    expect(joinImportStagingPath("", "imessage-ios", now)).toBe(
      "message-vault/staging-iphone-ios-260824-180509",
    );
  });
});
