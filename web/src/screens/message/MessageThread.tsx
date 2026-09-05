import Button from "../../components/Button";
import MessageBubble from "../../components/MessageBubble";
import { apiErrorMessage } from "../../lib/apiErrorMessage";
import type { Message, MessageAttachment } from "../../lib/types";
import { PAGE_SIZE } from "./useConversationMessages";

export default function MessageThread({
  messages,
  loading,
  error,
  findTerm,
  matchIds,
  activeMatch,
  activeYear,
  finding = false,
  footerLabel,
  offset,
  total,
  onPrevPage,
  onNextPage,
  onAttachmentClick,
}: {
  messages: Message[];
  loading: boolean;
  error: unknown;
  findTerm: string;
  matchIds: string[];
  activeMatch: number;
  /** The year the person is filtered to, or `null` while browsing all years. */
  activeYear: number | null;
  /** True while the rows are the find box's matches rather than the thread. */
  finding?: boolean;
  footerLabel: string;
  offset: number;
  total: number;
  onPrevPage: () => void;
  onNextPage: () => void;
  onAttachmentClick: (att: MessageAttachment, source: string) => void;
}) {
  const emptyText = finding
    ? "No messages match"
    : activeYear !== null
      ? `No messages in ${activeYear}`
      : "No messages in this conversation";
  return (
    <>
      <div className="flex-1 overflow-auto">
        {loading ? (
          <div className="p-4 text-[0.813rem] text-muted">Loading…</div>
        ) : error ? (
          <div className="p-4 text-[0.813rem] text-danger">
            {apiErrorMessage(error, "Could not load messages.")}
          </div>
        ) : messages.length === 0 ? (
          <div className="p-4 text-[0.813rem] text-muted">{emptyText}</div>
        ) : (
          messages.map((m) => (
            <MessageBubble
              key={m.id}
              message={m}
              highlight={findTerm.trim() || undefined}
              isActive={matchIds.length > 0 && matchIds[activeMatch] === String(m.id)}
              onAttachmentClick={onAttachmentClick}
            />
          ))
        )}
      </div>

      <div className="flex items-center justify-center gap-4 border-t border-border p-2 text-[0.813rem] text-muted">
        <Button onClick={onPrevPage} disabled={offset === 0 || loading} size="xs">
          Previous
        </Button>
        <span>{footerLabel}</span>
        <Button onClick={onNextPage} disabled={offset + PAGE_SIZE >= total || loading} size="xs">
          Next
        </Button>
      </div>
    </>
  );
}
