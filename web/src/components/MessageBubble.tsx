import type { Message, MessageAttachment } from "../lib/types";
import SmsBubble from "./messages/SmsBubble";
import ImessageBubble from "./messages/ImessageBubble";
import DiscordBubble from "./messages/DiscordBubble";
import WhatsAppBubble from "./messages/WhatsAppBubble";
import InstagramBubble from "./messages/InstagramBubble";

export default function MessageBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: (attachment: MessageAttachment) => void;
}) {
  switch (message.conversation.service?.toLowerCase()) {
    case "imessage":
    case "ios":
      return <ImessageBubble message={message} highlight={highlight} isActive={isActive} />;
    case "discord":
      return <DiscordBubble message={message} highlight={highlight} isActive={isActive} />;
    case "whatsapp":
      return <WhatsAppBubble message={message} highlight={highlight} isActive={isActive} />;
    case "instagram":
      return <InstagramBubble message={message} highlight={highlight} isActive={isActive} />;
    default:
      return <SmsBubble message={message} highlight={highlight} isActive={isActive} onAttachmentClick={onAttachmentClick} />;
  }
}
