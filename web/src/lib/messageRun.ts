import { calendarDayKey, parseInstant } from "./dateTimeFormat";
import type { MessageRow } from "./types";

export type MessageRunPosition = "single" | "first" | "middle" | "last";

export type MessageRunItem = {
  message: MessageRow;
  run: MessageRunPosition;
  showSender: boolean;
  showTimestamp: boolean;
};

const MAX_GAP_MS = 5 * 60 * 1000;

function instantMs(raw: string): number | null {
  const instant = parseInstant(raw);
  if (!instant) return null;
  return instant.date.getTime();
}

function senderKey(m: MessageRow): string {
  if (m.isFromMe) return "__me__";
  return m.sender?.trim() || m.senderName.trim() || "__unknown__";
}

/** Whether two adjacent messages belong in the same visual run. */
export function messagesFormRun(a: MessageRow, b: MessageRow): boolean {
  if (a.isAnnouncement || b.isAnnouncement) return false;
  if (a.isFromMe !== b.isFromMe) return false;
  if (senderKey(a) !== senderKey(b)) return false;
  if (calendarDayKey(a.timestamp) !== calendarDayKey(b.timestamp)) return false;
  const aMs = instantMs(a.timestamp);
  const bMs = instantMs(b.timestamp);
  if (aMs == null || bMs == null) return false;
  return Math.abs(bMs - aMs) <= MAX_GAP_MS;
}

/**
 * Annotate messages with run position for compact chat rendering.
 * Messages should already be in chronological order within a day section.
 */
export function annotateMessageRuns(messages: MessageRow[]): MessageRunItem[] {
  return messages.map((message, i) => {
    const prev = i > 0 ? messages[i - 1]! : null;
    const next = i < messages.length - 1 ? messages[i + 1]! : null;
    const withPrev = prev != null && messagesFormRun(prev, message);
    const withNext = next != null && messagesFormRun(message, next);

    let run: MessageRunPosition;
    if (withPrev && withNext) run = "middle";
    else if (withPrev) run = "last";
    else if (withNext) run = "first";
    else run = "single";

    const showSender = !message.isFromMe && !message.isAnnouncement && !withPrev;
    const showTimestamp = !message.isAnnouncement && !withNext;

    return { message, run, showSender, showTimestamp };
  });
}
