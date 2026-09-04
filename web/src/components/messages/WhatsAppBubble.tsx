import MessageAttachments from "../MessageAttachments";
import {
  type MessageBubbleProps,
  ServiceBubbleShell,
  ServiceMessageText,
} from "./chatBubbleShared";

export default function WhatsAppBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const mine = message.is_from_me;

  return (
    <ServiceBubbleShell
      message={message}
      isActive={isActive}
      senderClassName="text-[var(--whatsapp-brand)]"
    >
      <ServiceMessageText text={message.text || ""} highlight={highlight} mine={mine} />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </ServiceBubbleShell>
  );
}
