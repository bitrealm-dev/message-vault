import { describe, expect, it } from "vitest";
import { whatsappExtractFields } from "./whatsappExtractFields";

describe("whatsappExtractFields", () => {
  it("sends Android key and omits empty wa, leftover business, and backup password", () => {
    expect(
      whatsappExtractFields({
        source: "whatsapp-android",
        attachmentMedia: "convert",
        maxResolution: "1080p",
        maxFps: "30",
        minSizeMb: "20",
        key: "deadbeef",
        wa: "  ",
        media: "/tmp/WhatsApp",
        db: "/tmp/msgstore.db",
        business: true,
      }),
    ).toEqual({
      attachment_media: "convert",
      media_max_resolution: "1080p",
      media_max_fps: "30",
      media_min_size: "20M",
      whatsapp_key: "deadbeef",
      whatsapp_media: "/tmp/WhatsApp",
      whatsapp_db: "/tmp/msgstore.db",
    });
  });

  it("omits leftover Android media and db on iPhone", () => {
    expect(
      whatsappExtractFields({
        source: "whatsapp-ios",
        attachmentMedia: "copy",
        maxResolution: "720p",
        maxFps: "30",
        minSizeMb: "20",
        key: "",
        wa: "/backups/ContactsV2.sqlite",
        media: "/tmp/WhatsApp",
        db: "/tmp/msgstore.db",
        business: false,
      }),
    ).toEqual({
      attachment_media: "copy",
      media_max_resolution: "720p",
      media_max_fps: "30",
      media_min_size: "20M",
      whatsapp_wa: "/backups/ContactsV2.sqlite",
    });
  });

  it("sets iPhone business and omits leftover key", () => {
    expect(
      whatsappExtractFields({
        source: "whatsapp-ios",
        attachmentMedia: "copy",
        maxResolution: "720p",
        maxFps: "30",
        minSizeMb: "20",
        key: "leftover",
        wa: "/backups/ContactsV2.sqlite",
        media: "",
        db: "",
        business: true,
      }),
    ).toEqual({
      attachment_media: "copy",
      media_max_resolution: "720p",
      media_max_fps: "30",
      media_min_size: "20M",
      whatsapp_wa: "/backups/ContactsV2.sqlite",
      whatsapp_business: true,
    });
  });
});
