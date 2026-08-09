"use client";

import type { MessageRunPosition } from "@/lib/messageRun";
import type { MessageRow } from "@/lib/types";
import { highlightText } from "./highlightText";
import { MessageAttachments } from "./MessageAttachments";
import { useDateTimeFormat } from "./useDateTimeFormat";

function bubbleRadius(isFromMe: boolean, run: MessageRunPosition): string {
  if (isFromMe) {
    switch (run) {
      case "first":
        return "rounded-2xl rounded-br-md";
      case "middle":
        return "rounded-2xl rounded-r-md";
      case "last":
        return "rounded-2xl rounded-tr-md";
      default:
        return "rounded-2xl rounded-br-md";
    }
  }
  switch (run) {
    case "first":
      return "rounded-2xl rounded-bl-md";
    case "middle":
      return "rounded-2xl rounded-l-md";
    case "last":
      return "rounded-2xl rounded-tl-md";
    default:
      return "rounded-2xl rounded-bl-md";
  }
}

export function MessageBubble({
  message,
  highlightTerms = [],
  run = "single",
  showSender = true,
  showTimestamp = true,
}: {
  message: MessageRow;
  highlightTerms?: string[];
  run?: MessageRunPosition;
  showSender?: boolean;
  showTimestamp?: boolean;
}) {
  const { formatTime } = useDateTimeFormat();
  const align = message.isFromMe ? "items-end" : "items-start";
  const bubble = message.isFromMe
    ? `bg-sent text-sent-text ${bubbleRadius(true, run)}`
    : `bg-received text-received-text ${bubbleRadius(false, run)}`;
  const stackGap =
    run === "middle" || run === "first" ? "mb-0.5" : "mb-0";

  if (message.isAnnouncement) {
    return (
      <div
        id={`msg-${message.id}`}
        className="my-2 text-center text-[12px] text-muted"
        data-timestamp={message.timestamp}
      >
        {highlightText(message.body || "Announcement", highlightTerms)}
      </div>
    );
  }

  return (
    <div
      id={`msg-${message.id}`}
      className={`flex flex-col ${align} ${stackGap}`}
      data-timestamp={message.timestamp}
    >
      {showSender && !message.isFromMe && (
        <span className="mb-0.5 px-1 text-[12px] text-muted">
          {message.senderName}
        </span>
      )}
      <div
        className={`max-w-[min(75%,28rem)] px-3 py-2 text-[14px] leading-snug ${bubble}`}
      >
        {message.body && (
          <p className="whitespace-pre-wrap break-words">
            {highlightText(message.body, highlightTerms)}
          </p>
        )}
        <MessageAttachments
          source={message.source}
          attachments={message.attachments}
          hasBody={Boolean(message.body)}
        />
      </div>
      {showTimestamp ? (
        <span className="mt-0.5 px-1 text-[12px] text-muted">
          {formatTime(message.timestamp)}
        </span>
      ) : null}
    </div>
  );
}
