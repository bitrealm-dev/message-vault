import MessageAttachments from "../MessageAttachments";
import {
  formatMessageTime,
  senderName,
  ServiceMessageText,
  ServiceRow,
  type MessageBubbleProps,
} from "./chatBubbleShared";

export default function WhatsAppBubble({
  message,
  highlight,
  isActive,
  onAttachmentClick,
}: MessageBubbleProps) {
  const mine = message.is_from_me;

  return (
    <ServiceRow messageId={message.id} isActive={isActive}>
      <div className={`mb-1 flex gap-2 ${mine ? "justify-end" : "justify-start"}`}>
        <span className="text-[0.75rem] font-semibold text-[#075e54]">
          {senderName(message)}
        </span>
        <span className="text-[0.75rem] text-muted">
          {formatMessageTime(message.timestamp)}
        </span>
      </div>

      {message.reply_to_message && (
        <div
          className={`mb-1 rounded border-l-[3px] border-l-[#25d366] bg-hover px-2 py-1 text-[0.75rem] text-muted ${
            mine ? "text-right" : "text-left"
          }`}
        >
          <span className="font-semibold">{message.reply_to_message.sender_name}</span>:{" "}
          {message.reply_to_message.body_preview}
        </div>
      )}

      {message.deleted_indicator ? (
        <div
          className={`text-[0.875rem] italic text-muted ${
            mine ? "text-right" : "text-left"
          }`}
        >
          This message was deleted
        </div>
      ) : (
        <ServiceMessageText
          text={message.text || ""}
          highlight={highlight}
          mine={mine}
        />
      )}

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </ServiceRow>
  );
}
