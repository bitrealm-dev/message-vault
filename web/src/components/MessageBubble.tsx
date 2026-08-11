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

function normalizeToken(value: string | null | undefined): string {
  return (value || "").trim().toLowerCase();
}

/** Pick bubble style from each message's transport and import source. */
function resolveBubbleKind(message: Message): "imessage" | "discord" | "whatsapp" | "instagram" | "sms" {
  const service = normalizeToken(message.service);
  const source = normalizeToken(message.source);

  if (
    service === "imessage" ||
    service === "ios" ||
    service.includes("imessage") ||
    source === "imessage" ||
    source.startsWith("imessage") ||
    source.includes("iphone") ||
    source.includes("macos")
  ) {
    return "imessage";
  }
  if (service === "discord" || source.includes("discord")) return "discord";
  if (service === "whatsapp" || source.includes("whatsapp")) return "whatsapp";
  if (service === "instagram" || source.includes("instagram")) return "instagram";
  return "sms";
}

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

  switch (resolveBubbleKind(message)) {
    case "imessage":
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
