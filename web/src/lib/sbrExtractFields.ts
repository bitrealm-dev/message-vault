import type { AttachmentMediaMode, ExtractConfig } from "./types";

/** Build the extract payload fields shared by iPhone and SMS Backup & Restore media options. */
export function mediaExtractFields(args: {
  attachmentMedia: AttachmentMediaMode;
  maxResolution: string;
  maxFps: string;
  minSizeMb: string;
}): Pick<
  ExtractConfig,
  "attachment_media" | "media_max_resolution" | "media_max_fps" | "media_min_size"
> {
  return {
    attachment_media: args.attachmentMedia,
    media_max_resolution: args.maxResolution,
    media_max_fps: args.maxFps,
    media_min_size: `${args.minSizeMb.trim() || "20"}M`,
  };
}

/** Build extract options for SMS Backup & Restore (owner phones required). */
export function sbrExtractFields(args: {
  attachmentMedia: AttachmentMediaMode;
  maxResolution: string;
  maxFps: string;
  minSizeMb: string;
  ownerPhones: string[];
  obfuscate: boolean;
}): Pick<
  ExtractConfig,
  | "attachment_media"
  | "media_max_resolution"
  | "media_max_fps"
  | "media_min_size"
  | "owner_phones"
  | "obfuscate"
> {
  return {
    ...mediaExtractFields(args),
    owner_phones: args.ownerPhones,
    obfuscate: args.obfuscate,
  };
}
