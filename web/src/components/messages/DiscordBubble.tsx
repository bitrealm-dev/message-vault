import MessageAttachments from "../MessageAttachments";
import {
  type MessageBubbleProps,
  ServiceBubbleShell,
  ServiceMessageText,
  tapbackGroups,
} from "./chatBubbleShared";

export default function DiscordBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const mine = message.is_from_me;
  const tapbacks = tapbackGroups(message);

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

      {tapbacks.length > 0 && (
        <div className="mt-1 flex gap-1.5">
          {tapbacks.map((t) => (
            <span
              key={t.emoji}
              title={t.senderNames.join(", ")}
              className="rounded bg-border px-1 py-0.5 text-[0.75rem]"
            >
              {t.emoji} {t.count}
            </span>
          ))}
        </div>
      )}
    </ServiceBubbleShell>
  );
}
