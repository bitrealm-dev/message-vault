import Button from "../../components/Button";
import MessageBubble from "../../components/MessageBubble";
import type { Message, MessageAttachment } from "../../lib/types";
import { PAGE_SIZE } from "./useConversationMessages";

export default function MessageThread({
  messages,
  loading,
  findTerm,
  matchIds,
  activeMatch,
  yearMode,
  footerLabel,
  offset,
  total,
  onPrevPage,
  onNextPage,
  onAttachmentClick,
}: {
  messages: Message[];
  loading: boolean;
  findTerm: string;
  matchIds: string[];
  activeMatch: number;
  yearMode: boolean;
  footerLabel: string;
  offset: number;
  total: number;
  onPrevPage: () => void;
  onNextPage: () => void;
  onAttachmentClick: (att: MessageAttachment, source: string) => void;
}) {
  return (
    <>
      <div className="flex-1 overflow-auto">
        {loading ? (
          <div className="p-4 text-[0.813rem] text-muted">Loading…</div>
        ) : (
          messages.map((m) => (
            <MessageBubble
              key={m.id}
              message={m}
              highlight={findTerm.trim() || undefined}
              isActive={matchIds.length > 0 && matchIds[activeMatch] === m.id}
              onAttachmentClick={onAttachmentClick}
            />
          ))
        )}
      </div>

      <div className="flex items-center justify-center gap-4 border-t border-border p-2 text-[0.813rem] text-muted">
        {!yearMode && (
          <Button
            onClick={onPrevPage}
            disabled={offset === 0 || loading}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Previous
          </Button>
        )}
        <span>{footerLabel}</span>
        {!yearMode && (
          <Button
            onClick={onNextPage}
            disabled={offset + PAGE_SIZE >= total || loading}
            className="!px-3 !py-1 !text-[0.813rem]"
          >
            Next
          </Button>
        )}
      </div>
    </>
  );
}
