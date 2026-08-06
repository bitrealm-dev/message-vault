import type { Message } from "../lib/types";
import SmsBubble from "./messages/SmsBubble";
import ImessageBubble from "./messages/ImessageBubble";
import DiscordBubble from "./messages/DiscordBubble";

export default function MessageBubble({
  message,
  highlight,
  isActive,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
}) {
  switch (message.conversation.service?.toLowerCase()) {
    case "imessage":
    case "ios":
      return <ImessageBubble message={message} highlight={highlight} isActive={isActive} />;
    case "discord":
      return <DiscordBubble message={message} highlight={highlight} isActive={isActive} />;
    case "whatsapp":
      // Fall through to base for now — WhatsApp renderer in Task 8
    default:
      return <SmsBubble message={message} highlight={highlight} isActive={isActive} />;
  }
}
