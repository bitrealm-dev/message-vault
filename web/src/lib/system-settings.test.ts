import { beforeEach, describe, expect, it } from "vitest";
import {
  defaultImportStagingDir,
  isUsableImportStagingParent,
  joinImportStagingPath,
  resolveImportStagingDir,
} from "./system-settings";

const mem = new Map<string, string>();

beforeEach(() => {
  mem.clear();
  (globalThis as { localStorage?: Storage }).localStorage = {
    getItem: (k) => mem.get(k) ?? null,
    setItem: (k, v) => {
      mem.set(k, String(v));
    },
    removeItem: (k) => {
      mem.delete(k);
    },
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  };
});

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

  it("joins message-vault under a Unix root home", () => {
    expect(defaultImportStagingDir("/")).toBe("/message-vault");
  });
});

describe("isUsableImportStagingParent", () => {
  it("accepts an absolute folder", () => {
    expect(isUsableImportStagingParent("/data/imports")).toBe(true);
  });

  it("rejects the filesystem root", () => {
    expect(isUsableImportStagingParent("/")).toBe(false);
    expect(isUsableImportStagingParent("///")).toBe(false);
  });

  it("rejects a relative folder", () => {
    expect(isUsableImportStagingParent("message-vault")).toBe(false);
    expect(isUsableImportStagingParent("")).toBe(false);
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

  it("keeps a Unix root when stripping trailing slashes", () => {
    expect(joinImportStagingPath("/", "imessage-ios", now)).toBe(
      "/staging-iphone-ios-260824-180509",
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

  it("joins a saved custom parent without nesting message-vault", async () => {
    localStorage.setItem("mv-vault-working-dir", "/data/imports");
    await expect(resolveImportStagingDir("/backup", "imessage-ios")).resolves.toMatch(
      /^\/data\/imports\/staging-iphone-ios-\d{6}-\d{6}$/,
    );
  });

  it("ignores a saved filesystem root and fails without a home directory", async () => {
    localStorage.setItem("mv-vault-working-dir", "/");
    await expect(resolveImportStagingDir("/backup", "imessage-ios")).rejects.toThrow(
      /home directory/i,
    );
  });

  it("ignores a saved relative parent and fails without a home directory", async () => {
    localStorage.setItem("mv-vault-working-dir", "message-vault");
    await expect(resolveImportStagingDir("/backup", "imessage-ios")).rejects.toThrow(
      /home directory/i,
    );
  });
});
