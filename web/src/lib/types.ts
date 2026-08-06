export interface Participant {
  name: string | null;
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
}

export interface MessageParticipant {
  handle: string;
  name_hint: string | null;
  contact_id: string | null;
}

export interface MessageConversation {
  id: string;
  chat_identifier: string;
  service: string | null;
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
}

export interface MessageTapback {
  part_index: number;
  kind: string;
  emoji: string | null;
  is_from_me: boolean;
  sender: string | null;
}

export interface Message {
  id: string;
  source: string;
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
}

export interface ExtractConfig {
  source: string;
  path: string;
  output_dir: string;
}

export interface ExtractErrorEvent {
  detail: string;
  user_message?: string;
}
