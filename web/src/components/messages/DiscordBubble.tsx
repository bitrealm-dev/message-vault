import type { ReactNode } from "react";
import type { Message, MessageAttachment } from "../../lib/types";
import MessageAttachments from "../MessageAttachments";

function highlightText(text: string, term: string): ReactNode[] {
  const t = term.trim().toLowerCase();
  if (!t) return [text];
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (true) {
    const idx = rest.toLowerCase().indexOf(t);
    if (idx === -1) {
      out.push(rest);
      break;
    }
    if (idx > 0) out.push(rest.slice(0, idx));
    out.push(
      <mark key={key++} className="rounded-sm bg-search-mark px-px">
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}

function senderName(m: Message): string {
  if (m.is_from_me) return "Me";
  if (m.sender) {
    const p = m.conversation.participants.find((x) => x.handle === m.sender);
    return p?.name_hint || m.sender;
  }
  const p = m.conversation.participants[0];
  return p ? p.name_hint || p.handle : "Unknown";
}

export default function DiscordBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: {
  message: Message;
  highlight?: string;
  isActive?: boolean;
  onAttachmentClick?: (attachment: MessageAttachment, source: string) => void;
}) {
  const time = new Date(message.timestamp).toLocaleString([], {
    month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
  });
  const mine = message.is_from_me;

  return (
    <div
      id={`msg-${message.id}`}
      className={`border-b border-border px-6 py-2 ${
        isActive ? "bg-search-active" : "bg-transparent"
      }`}
    >
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
        <span className="text-[0.688rem] text-muted">{time}</span>
      </div>

      <div
        className={`whitespace-pre-wrap text-[0.875rem] leading-[1.5] text-text ${
          mine ? "text-right" : "text-left"
        }`}
      >
        {highlight ? highlightText(message.text || "", highlight) : message.text || ""}
      </div>

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
    </div>
  );
}
