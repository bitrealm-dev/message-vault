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
  const mine = message.is_from_me;
  const group = isGroupConversation(message);
  const body = (message.text || "").trim();
  const hasAttachments = message.attachments.length > 0;

  const footer = hasAttachments ? (
    <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
  ) : undefined;

  return (
    <ChatBubbleRow
      messageId={String(message.id)}
      mine={mine}
      isActive={isActive}
      palette="imessage"
      showSender={!mine && group}
      senderLabel={senderName(message)}
      timeLabel={time}
      footer={footer}
    >
      {bubbleBody(body, highlight)}
    </ChatBubbleRow>
  );
}
