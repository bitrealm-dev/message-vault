"use client";

import { calendarDayKey } from "@/lib/dateTimeFormat";
import { annotateMessageRuns } from "@/lib/messageRun";
import type { MessageRow } from "@/lib/types";
import { Fragment, useMemo } from "react";
import { MessageBubble } from "./MessageBubble";
import { useDateTimeFormat } from "./useDateTimeFormat";

function DateSeparator({ label }: { label: string }) {
  return (
    <div className="my-3 flex items-center gap-3" role="separator">
      <span aria-hidden className="h-px min-w-6 flex-1 bg-border/70" />
      <span className="shrink-0 text-[12px] font-medium tracking-wide text-muted tabular-nums">
        {label}
      </span>
      <span aria-hidden className="h-px min-w-6 flex-1 bg-border/70" />
    </div>
  );
}

/** Renders messages with day separators and compact same-sender runs. */
export function MessageList({
  messages,
  highlightTerms = [],
}: {
  messages: MessageRow[];
  highlightTerms?: string[];
}) {
  const { formatDate } = useDateTimeFormat();
  const annotated = useMemo(() => annotateMessageRuns(messages), [messages]);

  return (
    <>
      {annotated.map((item, i) => {
        const day = calendarDayKey(item.message.timestamp);
        const prev =
          i > 0 ? calendarDayKey(annotated[i - 1]!.message.timestamp) : null;
        const showDate = day != null && day !== prev;
        return (
          <Fragment key={item.message.id}>
            {showDate ? (
              <DateSeparator label={formatDate(item.message.timestamp)} />
            ) : null}
            <MessageBubble
              message={item.message}
              highlightTerms={highlightTerms}
              run={item.run}
              showSender={item.showSender}
              showTimestamp={item.showTimestamp}
            />
          </Fragment>
        );
      })}
    </>
  );
}
