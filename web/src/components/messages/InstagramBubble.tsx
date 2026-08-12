import MessageAttachments from "../MessageAttachments";
import {
  formatMessageTime,
  senderName,
  ServiceMessageText,
  ServiceRow,
  type MessageBubbleProps,
} from "./chatBubbleShared";

export default function InstagramBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const mine = message.is_from_me;

  return (
    <ServiceRow messageId={message.id} isActive={isActive}>
      <div className={`mb-1 flex gap-2 ${mine ? "justify-end" : "justify-start"}`}>
        <span className="text-[0.75rem] font-semibold text-[#e4405f]">
          {senderName(message)}
        </span>
        <span className="text-[0.75rem] text-muted">
          {formatMessageTime(message.timestamp)}
        </span>
        {message.is_story_reply && (
          <span className="text-[0.688rem] text-[#e4405f]">Story reply</span>
        )}
        {message.forwarded && (
          <span className="text-[0.688rem] text-muted">Forwarded</span>
        )}
      </div>

      <ServiceMessageText
        text={message.text || ""}
        highlight={highlight}
        mine={mine}
      />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </ServiceRow>
  );
}
