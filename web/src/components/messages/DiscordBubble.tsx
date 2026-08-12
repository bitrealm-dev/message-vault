import MessageAttachments from "../MessageAttachments";
import {
  formatMessageTime,
  senderName,
  ServiceMessageText,
  ServiceRow,
  type MessageBubbleProps,
} from "./chatBubbleShared";

export default function DiscordBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const mine = message.is_from_me;

  return (
    <ServiceRow messageId={message.id} isActive={isActive}>
      <div
        className={`mb-1 flex items-center gap-2 ${
          mine ? "justify-end" : "justify-start"
        }`}
      >
        <span
          className="text-[0.75rem] font-semibold"
          style={{ color: message.role_color || "#5865f2" }}
        >
          {senderName(message)}
        </span>
        <span className="text-[0.688rem] text-muted">
          {formatMessageTime(message.timestamp)}
        </span>
      </div>

      <ServiceMessageText
        text={message.text || ""}
        highlight={highlight}
        mine={mine}
      />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />

      {message.embeds && message.embeds.length > 0 && message.embeds.map((embed, i) => (
        <div
          key={i}
          className="mt-2 rounded-r-[4px] border-l-4 border-l-[#5865f2] bg-hover px-3 py-2"
        >
          {embed.title && (
            <div className="mb-0.5 text-[0.813rem] font-semibold">
              {embed.url ? <a href={embed.url} className="text-accent">{embed.title}</a> : embed.title}
            </div>
          )}
          {embed.description && (
            <div className="text-[0.813rem] text-muted">{embed.description}</div>
          )}
        </div>
      ))}

      {message.reactions && message.reactions.length > 0 && (
        <div className="mt-1 flex gap-1.5">
          {message.reactions.map((r, i) => (
            <span
              key={i}
              className="rounded bg-border px-1 py-0.5 text-[0.75rem]"
            >
              {r.emoji} {r.count}
            </span>
          ))}
        </div>
      )}
    </ServiceRow>
  );
}
