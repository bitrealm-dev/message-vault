import MessageAttachments from "../MessageAttachments";
import {
  type MessageBubbleProps,
  ServiceBubbleShell,
  ServiceMessageText,
} from "./chatBubbleShared";

export default function DiscordBubble({
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
      senderStyle={{ color: "var(--discord-brand)" }}
      timeClassName="text-[0.688rem] text-muted"
      headerAlignClassName="items-center"
    >
      <ServiceMessageText text={message.text || ""} highlight={highlight} mine={mine} />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </ServiceBubbleShell>
  );
}
