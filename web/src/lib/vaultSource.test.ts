import { describe, expect, it } from "vitest";
import { IMESSAGE_METHODS, IMESSAGE_SOURCE_ID } from "./imessageImport";
import { importSessionCreateBody, vaultSourceForMethod } from "./vaultSource";
import { WHATSAPP_METHODS, WHATSAPP_SOURCE_ID } from "./whatsappImport";

describe("vaultSourceForMethod", () => {
  it("maps each iMessage method id to imessage", () => {
    expect(IMESSAGE_METHODS.map((m) => m.id)).toEqual([
      "imessage-macos",
      "imessage-ios",
      "imessage-jailbreak",
    ]);
    for (const method of IMESSAGE_METHODS) {
      expect(vaultSourceForMethod(method.id)).toBe(IMESSAGE_SOURCE_ID);
    }
  });

  it("maps each WhatsApp method id to whatsapp", () => {
    expect(WHATSAPP_METHODS.map((m) => m.id)).toEqual([
      "whatsapp-android",
      "whatsapp-ios",
    ]);
    for (const method of WHATSAPP_METHODS) {
      expect(vaultSourceForMethod(method.id)).toBe(WHATSAPP_SOURCE_ID);
    }
  });

  it("leaves sms-backup-restore unchanged", () => {
    expect(vaultSourceForMethod("sms-backup-restore")).toBe("sms-backup-restore");
  });

  it("returns an unknown string unchanged", () => {
    expect(vaultSourceForMethod("not-a-real-source")).toBe("not-a-real-source");
  });
});

describe("importSessionCreateBody", () => {
  it("sends imessage when the form method is imessage-ios", () => {
    expect(importSessionCreateBody("imessage-ios")).toEqual({
      source: "imessage",
      tool: "message-vault-io",
      mode: "append",
    });
  });

  it("sends whatsapp when the form method is whatsapp-android", () => {
    expect(importSessionCreateBody("whatsapp-android")).toEqual({
      source: "whatsapp",
      tool: "message-vault-io",
      mode: "append",
    });
  });

  it("sends sms-backup-restore unchanged", () => {
    expect(importSessionCreateBody("sms-backup-restore").source).toBe(
      "sms-backup-restore",
    );
  });
});
