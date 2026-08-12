import type { Conversation } from "./types";

export type MessagesLocationState = {
  conversation?: Conversation;
  openContactId?: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isConversation(value: unknown): value is Conversation {
  if (!isRecord(value)) return false;
  return typeof value.id === "string" && value.id.length > 0;
}

/** Narrow React Router location state for message routes. */
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

  if (out.conversation === undefined && out.openContactId === undefined) {
    return null;
  }

  return out;
}
