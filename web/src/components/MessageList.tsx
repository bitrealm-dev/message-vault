"use client";

import { calendarDayKey } from "@/lib/dateTimeFormat";
import type { MessageRow } from "@/lib/types";
import { Fragment } from "react";
import { MessageBubble } from "./MessageBubble";
import { useDateTimeFormat } from "./useDateTimeFormat";

function DateSeparator({ label }: { label: string }) {
  return (
    <div className="my-4 text-center text-[13px] font-semibold tracking-wide text-text">
      {label}
    </div>
  );
}

/** Renders messages with iMessage-style centered date separators on day changes. */
export function MessageList({
  messages,
  highlightTerms = [],
}: {
  messages: MessageRow[];
  highlightTerms?: string[];
}) {
  const { formatDate } = useDateTimeFormat();

  return (
    <>
      {messages.map((m, i) => {
        const day = calendarDayKey(m.timestamp);
        const prev =
          i > 0 ? calendarDayKey(messages[i - 1]!.timestamp) : null;
        const showDate = day != null && day !== prev;
        return (
          <Fragment key={m.id}>
            {showDate ? (
              <DateSeparator label={formatDate(m.timestamp)} />
            ) : null}
            <MessageBubble message={m} highlightTerms={highlightTerms} />
          </Fragment>
        );
      })}
    </>
  );
}
