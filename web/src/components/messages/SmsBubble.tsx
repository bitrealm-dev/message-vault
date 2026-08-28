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

/** SMS / MMS / RCS / Android Messages — green sent bubbles. */
export default function SmsBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const time = formatMessageTime(message.timestamp, true);
  const useAliases = useNameAliases();
  const mine = message.is_from_me;
  const group = isGroupConversation(message);
  const body = (message.text || "").trim();
  const service = message.service?.trim() || message.source?.trim();
  const hasAttachments = message.attachments.length > 0;

  return (
    <ChatBubbleRow
      messageId={message.id}
      mine={mine}
      isActive={isActive}
      palette="sms"
      showSender={!mine && group}
      senderLabel={senderName(message, useAliases)}
      timeLabel={time}
      meta={service ? <span className="uppercase tracking-[0.04em]">{service}</span> : null}
      footer={
        hasAttachments ? (
          <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
        ) : undefined
      }
    >
      {bubbleBody(body, highlight)}
    </ChatBubbleRow>
  );
}
