import { describe, it, expect } from "vitest";
import {
  HANDLE_SERVICES,
  HANDLE_SERVICE_OPTIONS,
  CONTACT_IDENTITY_SERVICES,
  inferService,
  formatHandleServiceLabel,
  handleServiceSelectValue,
} from "./handleService";

describe("handleService", () => {
  it("lists phone, email, whatsapp for profile/onboarding", () => {
    expect([...HANDLE_SERVICES]).toEqual(["phone", "email", "whatsapp"]);
    expect(HANDLE_SERVICE_OPTIONS.map((o) => o.value)).toEqual([
      "phone",
      "email",
      "whatsapp",
    ]);
  });

  it("lists phone and whatsapp for contact identity picker", () => {
    expect([...CONTACT_IDENTITY_SERVICES]).toEqual(["phone", "whatsapp"]);
  });

  it("infers email and phone from handle when service empty", () => {
    expect(inferService("a@b.com", null)).toBe("email");
    expect(inferService("+1 (555) 123-4567", "")).toBe("phone");
    expect(inferService("alice", null)).toBe("unknown");
  });

  it("prefers an explicit service over inference", () => {
    expect(inferService("a@b.com", "WhatsApp")).toBe("whatsapp");
  });

  it("formats user-facing labels for the handles table", () => {
    expect(formatHandleServiceLabel("x", "imessage")).toBe("Text message");
    expect(formatHandleServiceLabel("x", "whatsapp")).toBe("WhatsApp");
    expect(formatHandleServiceLabel("a@b.com", null)).toBe("Email");
    expect(formatHandleServiceLabel("x", null)).toBe("—");
  });

  it("maps inferred services onto contact-identity select values", () => {
    expect(handleServiceSelectValue("x", "whatsapp")).toBe("whatsapp");
    expect(handleServiceSelectValue("a@b.com", null)).toBe("phone");
  });
});
