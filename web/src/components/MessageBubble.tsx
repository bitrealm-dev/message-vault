import type { Message, MessageAttachment } from "../lib/types";
import SmsBubble from "./messages/SmsBubble";
import ImessageBubble from "./messages/ImessageBubble";
import DiscordBubble from "./messages/DiscordBubble";
import WhatsAppBubble from "./messages/WhatsAppBubble";
import InstagramBubble from "./messages/InstagramBubble";

export type AttachmentClickHandler = (
  attachment: MessageAttachment,
  source: string,
) => void;

export default function MessageBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: AttachmentClickHandler;
}) {
  const props = { message, highlight, isActive, onAttachmentClick };

  switch (message.conversation.service?.toLowerCase()) {
    case "imessage":
    case "ios":
      return <ImessageBubble {...props} />;
    case "discord":
      return <DiscordBubble {...props} />;
    case "whatsapp":
      return <WhatsAppBubble {...props} />;
    case "instagram":
      return <InstagramBubble {...props} />;
    default:
      return <SmsBubble {...props} />;
  }
}
