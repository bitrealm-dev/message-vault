import { useNameAliases } from "../../lib/useNameAliases";
import MessageAttachments from "../MessageAttachments";
import {
  bubbleBody,
  ChatBubbleRow,
  formatMessageTime,
  isGroupConversation,
  type MessageBubbleProps,
  senderName,
} from "./chatBubbleShared";

export default function ImessageBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const time = formatMessageTime(message.timestamp);
  const useAliases = useNameAliases();
  const mine = message.is_from_me;
  const group = isGroupConversation(message);
  const body = (message.text || "").trim();
  const hasReactions = Boolean(message.reactions && message.reactions.length > 0);
  const hasAttachments = message.attachments.length > 0;

  const footer =
    hasAttachments || hasReactions ? (
      <>
        <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
        {hasReactions ? (
          <div className={`flex flex-wrap gap-1 ${hasAttachments ? "mt-1" : "mt-0"}`}>
            {message.reactions?.map((r) => (
              <span
                key={r.emoji}
                className="rounded-full border border-border bg-elevated px-[0.35rem] py-[0.1rem] text-[0.75rem] text-text"
              >
                {r.emoji} {r.count}
              </span>
            ))}
          </div>
        ) : null}
      </>
    ) : undefined;

  return (
    <ChatBubbleRow
      messageId={String(message.id)}
      mine={mine}
      isActive={isActive}
      palette="imessage"
      showSender={!mine && group}
      senderLabel={senderName(message, useAliases)}
      timeLabel={time}
      meta={
        <>
          {message.effect ? (
            <span className="italic text-[var(--imessage-effect)]">{message.effect}</span>
          ) : null}
          {message.edit_history && message.edit_history.length > 0 ? (
            <span className="italic">Edited</span>
          ) : null}
        </>
      }
      footer={footer}
    >
      {bubbleBody(body, highlight)}
    </ChatBubbleRow>
  );
}
