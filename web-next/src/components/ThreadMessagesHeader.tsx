"use client";

import { MessageIcon, PaperclipIcon } from "./icons";
import { useDateTimeFormat } from "./useDateTimeFormat";

/** Shared header above a year / thread message list. */
export function ThreadMessagesHeader({
  title,
  messageCount,
  dateStart,
  dateEnd,
  attachmentCount,
  largeTitle = false,
}: {
  title: string;
  messageCount: number;
  dateStart: string;
  dateEnd: string;
  attachmentCount?: number;
  /** Year headers use a much larger title. */
  largeTitle?: boolean;
}) {
  const { formatDateRange } = useDateTimeFormat();
  const dateLabel = formatDateRange(dateStart, dateEnd);

  return (
    <div className="mb-2 border-b border-border/60 pb-3 text-center">
      <div
        className={
          largeTitle
            ? "text-4xl font-semibold tracking-tight text-text tabular-nums"
            : "text-[15px] font-semibold text-text"
        }
      >
        {title}
      </div>
      <div className="mt-1.5 text-[13px] text-muted tabular-nums">{dateLabel}</div>
      <div className="mt-2 flex flex-wrap items-center justify-center gap-x-2 gap-y-1 text-[12px] text-muted">
        <span className="inline-flex items-center gap-1 tabular-nums">
          <MessageIcon className="size-3.5 shrink-0 opacity-80" />
          {messageCount.toLocaleString()}
        </span>
        {attachmentCount != null && (
          <>
            <span className="opacity-50">·</span>
            <span className="inline-flex items-center gap-1 tabular-nums">
              <PaperclipIcon className="size-3.5 shrink-0 opacity-80" />
              {attachmentCount.toLocaleString()}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
