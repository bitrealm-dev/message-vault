import type { Message, MessageAttachment } from "../../lib/types";
import MessageAttachments from "../MessageAttachments";
import {
  ChatBubbleRow,
  highlightText,
  isGroupConversation,
  senderName,
} from "./chatBubbleShared";

export default function ImessageBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: (attachment: MessageAttachment, source: string) => void;
}) {
  const time = new Date(message.timestamp).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
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
          <div
            style={{
              display: "flex",
              gap: "0.25rem",
              flexWrap: "wrap",
              marginTop: hasAttachments ? "0.25rem" : 0,
            }}
          >
            {message.reactions!.map((r, i) => (
              <span
                key={i}
                style={{
                  fontSize: "0.75rem",
                  background: "var(--elevated)",
                  border: "1px solid var(--border)",
                  padding: "0.1rem 0.35rem",
                  borderRadius: "999px",
                  color: "var(--text)",
                }}
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
      messageId={message.id}
      mine={mine}
      isActive={isActive}
      palette="imessage"
      showSender={!mine && group}
      senderLabel={senderName(message)}
      senderColor="var(--imessage-sent)"
      timeLabel={time}
      meta={
        <>
          {message.effect ? (
            <span style={{ color: "#8b5cf6", fontStyle: "italic" }}>{message.effect}</span>
          ) : null}
          {message.edit_history && message.edit_history.length > 0 ? (
            <span style={{ fontStyle: "italic" }}>Edited</span>
          ) : null}
        </>
      }
      footer={footer}
    >
      {body ? (highlight ? highlightText(body, highlight) : body) : undefined}
    </ChatBubbleRow>
  );
}
