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
      senderStyle={{ color: message.role_color || "#5865f2" }}
      timeClassName="text-[0.688rem] text-muted"
      headerAlignClassName="items-center"
    >
      <ServiceMessageText text={message.text || ""} highlight={highlight} mine={mine} />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />

      {message.embeds &&
        message.embeds.length > 0 &&
        message.embeds.map((embed) => (
          <div
            key={[embed.type, embed.url, embed.title, embed.description].filter(Boolean).join("|")}
            className="mt-2 rounded-r-[4px] border-l-4 border-l-[#5865f2] bg-hover px-3 py-2"
          >
            {embed.title && (
              <div className="mb-0.5 text-[0.813rem] font-semibold">
                {embed.url ? (
                  <a href={embed.url} className="text-accent">
                    {embed.title}
                  </a>
                ) : (
                  embed.title
                )}
              </div>
            )}
            {embed.description && (
              <div className="text-[0.813rem] text-muted">{embed.description}</div>
            )}
          </div>
        ))}

      {message.reactions && message.reactions.length > 0 && (
        <div className="mt-1 flex gap-1.5">
          {message.reactions.map((r) => (
            <span key={r.emoji} className="rounded bg-border px-1 py-0.5 text-[0.75rem]">
              {r.emoji} {r.count}
            </span>
          ))}
        </div>
      )}
    </ServiceBubbleShell>
  );
}
