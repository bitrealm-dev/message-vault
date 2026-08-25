import type { Conversation } from "./types";

/** Lightweight contact row carried on location state so the drawer can paint immediately. */
export type OpenContactPreview = {
  id: string;
  name: string;
  handles?: string[];
  handleCount?: number;
  groups?: string[];
};

export type MessagesLocationState = {
  conversation?: Conversation;
  openContactId?: string;
  openContactPreview?: OpenContactPreview;
};

/** True when the value is a plain object (not null or an array). */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** True when the value looks like a conversation with a non-empty id. */
function isConversation(value: unknown): value is Conversation {
  if (!isRecord(value)) return false;
  return typeof value.id === "string" && value.id.length > 0;
}

/** True when every element is a string. */
function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

/**
 * Parse a contact preview from location state.
 * Requires string `id` and `name`. Optional `handles`/`groups` must be string arrays;
 * optional `handleCount` must be a finite number. Returns null on any invalid field.
 */
function asOpenContactPreview(value: unknown): OpenContactPreview | null {
  if (!isRecord(value)) return null;
  if (typeof value.id !== "string" || typeof value.name !== "string") return null;

  const out: OpenContactPreview = { id: value.id, name: value.name };

  if ("handles" in value && value.handles !== undefined) {
    if (!isStringArray(value.handles)) return null;
    out.handles = value.handles;
  }

  if ("groups" in value && value.groups !== undefined) {
    if (!isStringArray(value.groups)) return null;
    out.groups = value.groups;
  }

  if ("handleCount" in value && value.handleCount !== undefined) {
    if (typeof value.handleCount !== "number" || !Number.isFinite(value.handleCount)) {
      return null;
    }
    out.handleCount = value.handleCount;
  }

  return out;
}

/** Read conversation and contact-drawer fields from React Router location state. */
export function asMessagesLocationState(state: unknown): MessagesLocationState | null {
  if (!isRecord(state)) return null;

  const out: MessagesLocationState = {};

  if ("conversation" in state && state.conversation !== undefined) {
    if (!isConversation(state.conversation)) return null;
    out.conversation = state.conversation;
  }

  if ("openContactId" in state && state.openContactId !== undefined) {
    if (typeof state.openContactId !== "string") return null;
    out.openContactId = state.openContactId;
  }

  if ("openContactPreview" in state && state.openContactPreview !== undefined) {
    const preview = asOpenContactPreview(state.openContactPreview);
    if (preview && out.openContactId !== undefined && preview.id === out.openContactId) {
      out.openContactPreview = preview;
    }
  }

  if (out.conversation === undefined && out.openContactId === undefined) {
    return null;
  }

  return out;
}
