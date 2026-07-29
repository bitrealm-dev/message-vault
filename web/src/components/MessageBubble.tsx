"use client";

import type { MessageRow } from "@/lib/types";
import type { ReactNode } from "react";
import { MessageAttachments } from "./MessageAttachments";
import { useDateTimeFormat } from "./useDateTimeFormat";

function highlightText(text: string, terms: string[]): ReactNode {
  const cleaned = terms.map((t) => t.trim()).filter(Boolean);
  if (cleaned.length === 0) return text;
  const pattern = cleaned
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("|");
  if (!pattern) return text;
  const re = new RegExp(`(${pattern})`, "gi");
  const parts = text.split(re);
  return parts.map((part, i) =>
    i % 2 === 1 ? (
      <mark
        key={i}
        className="rounded-sm bg-accent/35 px-0.5 text-inherit"
      >
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

export function MessageBubble({
  message,
  highlightTerms = [],
}: {
  message: MessageRow;
  highlightTerms?: string[];
}) {
  const { formatTime } = useDateTimeFormat();
  const align = message.isFromMe ? "items-end" : "items-start";
  const bubble = message.isFromMe
    ? "bg-sent text-sent-text rounded-2xl rounded-br-md"
    : "bg-received text-received-text rounded-2xl rounded-bl-md";

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
      className={`flex flex-col ${align}`}
      data-timestamp={message.timestamp}
    >
      {!message.isFromMe && (
        <span className="mb-0.5 px-1 text-[12px] text-muted">
          {message.senderName}
        </span>
      )}
      <div className={`max-w-[75%] px-3 py-2 text-[14px] leading-snug ${bubble}`}>
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
      <span className="mt-0.5 px-1 text-[12px] text-muted">
        {formatTime(message.timestamp)}
      </span>
    </div>
  );
}
