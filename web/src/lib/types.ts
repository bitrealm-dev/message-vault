export interface Participant {
  name: string | null;
  handle: string;
  service: string;
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

export interface Message {
  id: string;
  conversation_id: string;
  sender: Participant;
  body: string;
  sent_at: string;
  service: string;
  attachments: Attachment[];
  reply_to: string | null;
  is_deleted: boolean;
}

export interface Attachment {
  sha256: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
}

export interface PaginatedMessages {
  messages: Message[];
  total: number;
  offset: number;
  limit: number;
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
