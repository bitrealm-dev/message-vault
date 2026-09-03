import type { components } from "./vaultApi.types";

type Schema = components["schemas"];

/*
 * Shapes the vault returns come from the generated types, so a field renamed
 * on the server is a build error here rather than an empty screen. Shapes
 * below that the vault never sends — desktop command arguments, progress
 * events, and the per-app extras on a message — stay hand-written.
 */

/** One participant in a conversation, as the conversation list returns them. */
export type Participant = Schema["Participant"];

/** One conversation in the browse list. */
export type Conversation = Schema["ConversationSummary"];

/** One participant on a message the Export routes return. */
export type MessageParticipant = Schema["Participant"];

/** The conversation a message belongs to, as the Export routes return it. */
export type MessageConversation = Schema["ExportConversation"];

/** One attachment on a message. */
export type MessageAttachment = Schema["ExportAttachment"];

/** One tapback reaction on a message. */
export type MessageTapback = Schema["ExportTapback"];

export interface Reaction {
  emoji: string;
  count: number;
  users: string[]; // display names
}

export interface MessageRef {
  id: string;
  sender_name: string;
  body_preview: string;
}

export interface Embed {
  type: "image" | "video" | "link" | "rich";
  url?: string;
  title?: string;
  description?: string;
  thumbnail_url?: string;
}

export interface EditEntry {
  body: string;
  edited_at: string;
}

/**
 * One message as the Export routes return it, plus the per-app extras the
 * message bubbles render.
 *
 * The vault does not currently send any of the optional fields below: they are
 * not in its OpenAPI document, so the branches that render them never run. They
 * are kept typed rather than deleted so that removing those branches stays a
 * separate, reviewable change.
 */
export type Message = Schema["ExportMessage"] & {
  reactions?: Reaction[]; // iMessage tapbacks, Discord reactions
  reply_to_message?: MessageRef; // WhatsApp reply chains
  embeds?: Embed[]; // Discord embeds
  edit_history?: EditEntry[]; // iMessage edit history
  deleted_indicator?: boolean; // WhatsApp "this message was deleted"
  effect?: string; // iMessage screen effect
  role_color?: string; // Discord role color
  is_story_reply?: boolean; // Instagram story reply
  forwarded?: boolean; // Instagram forwarding indicator
};

export type AttachmentMediaMode = "copy" | "convert" | "compress" | "skip";

export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
  backup_password?: string;
  attachment_media?: AttachmentMediaMode;
  media_max_resolution?: string;
  media_max_fps?: string;
  media_min_size?: string;
  obfuscate?: boolean;
  /** Owner phone numbers for Android SMS exporters (repeatable). */
  owner_phones?: string[];
  /** Alternate folder for Attachments and StickerCache (Mac and jailbreak). */
  attachment_root?: string;
  /** Path to an Apple AddressBook file (Mac and jailbreak). */
  apple_contacts?: string;
  /** WhatsApp decryption key (file path or crypt15 hex). Not an Apple backup password. */
  whatsapp_key?: string;
  /** WhatsApp contacts database (`wa.db` or ContactsV2.sqlite). */
  whatsapp_wa?: string;
  /** WhatsApp media folder override. */
  whatsapp_media?: string;
  /** Explicit WhatsApp message database (`msgstore.db`). */
  whatsapp_db?: string;
  /** iPhone WhatsApp Business default files (`--business`). */
  whatsapp_business?: boolean;
  /** Continue an interrupted export in the same folder: previous output is
   * kept and conversations already written are skipped. */
  resume?: boolean;
}

export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}

export interface ImportProgressEvent {
  step: "parse" | "attachments" | "prepare" | "media" | "upload";
  done: number;
  total: number;
  bytes_done?: number;
  bytes_total?: number;
  status?: string;
}

export interface ImportIssueEvent {
  kind: "error" | "skip";
  step: "parse" | "attachments" | "prepare" | "media" | "upload";
  item: string;
  reason: string;
}
