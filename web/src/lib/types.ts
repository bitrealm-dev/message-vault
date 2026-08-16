export interface Participant {
  name: string | null;
  name_alias?: string | null;
  handle: string;
  service: string;
  contact_id: string | null;
}

export interface Conversation {
  id: string;
  participants: Participant[];
  message_count: number;
  last_message_at: string;
  date_range_start: string | null;
  date_range_end: string | null;
  service: string;
  is_group: boolean;
  label: string | null;
  /** Thread tags on this conversation. */
  tags?: string[];
}

export interface MessageParticipant {
  handle: string;
  name_alias: string | null;
  preferred_name?: string | null;
  contact_id: string | null;
}

export interface MessageConversation {
  id: string;
  chat_identifier: string;
  conversation_type: string;
  group_title: string | null;
  participants: MessageParticipant[];
}

export interface MessageAttachment {
  path: string | null;
  original_name: string | null;
  mime_type: string | null;
  sha256: string | null;
  is_sticker: boolean;
  transcription: string | null;
  /** Why the file bytes are missing (`too_large` / `file_missing`). Null when the file exists. */
  missing_reason?: string | null;
}

export interface MessageTapback {
  part_index: number;
  kind: string;
  emoji: string | null;
  is_from_me: boolean;
  sender: string | null;
}

export interface Reaction {
  emoji: string;
  count: number;
  users: string[];  // display names
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

export interface Message {
  id: string;
  source: string;
  service?: string | null;
  guid: string | null;
  timestamp: string;
  timestamp_utc: string | null;
  is_from_me: boolean;
  sender: string | null;
  subject: string | null;
  text: string | null;
  conversation: MessageConversation;
  attachments: MessageAttachment[];
  tapbacks: MessageTapback[];

  // Extra fields that only some messaging apps send.
  reactions?: Reaction[];        // iMessage tapbacks, Discord reactions
  reply_to_message?: MessageRef; // WhatsApp reply chains
  embeds?: Embed[];              // Discord embeds
  edit_history?: EditEntry[];    // iMessage edit history
  deleted_indicator?: boolean;   // WhatsApp "this message was deleted"
  effect?: string;               // iMessage screen effect
  role_color?: string;           // Discord role color
  is_story_reply?: boolean;      // Instagram story reply
  forwarded?: boolean;           // Instagram forwarding indicator
}

export type AttachmentMediaMode = "copy" | "convert" | "compress" | "skip";

/** How vault contacts fill in display names during import. */
export type ContactNameMode = "fill_missing" | "overwrite" | "as_is";

export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
  backup_password?: string;
  attachment_media?: AttachmentMediaMode;
  media_max_resolution?: string;
  media_max_fps?: string;
  media_min_size?: string;
  conversation_filter?: string;
  start_date?: string;
  end_date?: string;
  obfuscate?: boolean;
}

export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}

export interface ImportProgressEvent {
  step: "parse" | "convert" | "upload";
  done: number;
  total: number;
  status?: string;
}

export interface ImportIssueEvent {
  kind: "error" | "skip";
  step: "parse" | "convert" | "upload";
  item: string;
  reason: string;
}
