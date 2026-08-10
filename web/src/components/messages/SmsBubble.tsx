import type { Message, MessageAttachment } from "../../lib/types";
import MessageAttachments from "../MessageAttachments";
import {
  ChatBubbleRow,
  highlightText,
  isGroupConversation,
  senderName,
} from "./chatBubbleShared";

/** SMS / MMS / RCS / Android Messages — green sent bubbles. */
export default function SmsBubble({
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
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
  const mine = message.is_from_me;
  const group = isGroupConversation(message);
  const body = (message.text || "").trim();
  const service = message.conversation.service?.trim();
  const hasAttachments = message.attachments.length > 0;

  return (
    <ChatBubbleRow
      messageId={message.id}
      mine={mine}
      isActive={isActive}
      palette="sms"
      showSender={!mine && group}
      senderLabel={senderName(message)}
      timeLabel={time}
      meta={
        service ? (
          <span className="uppercase tracking-[0.04em]">{service}</span>
        ) : null
      }
      footer={
        hasAttachments ? (
          <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
        ) : undefined
      }
    >
      {body ? (highlight ? highlightText(body, highlight) : body) : undefined}
    </ChatBubbleRow>
  );
}
