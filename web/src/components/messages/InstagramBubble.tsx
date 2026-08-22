import MessageAttachments from "../MessageAttachments";
import {
  type MessageBubbleProps,
  ServiceBubbleShell,
  ServiceMessageText,
} from "./chatBubbleShared";

export default function InstagramBubble({
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
      senderClassName="text-[#e4405f]"
      headerExtra={
        <>
          {message.is_story_reply && (
            <span className="text-[0.688rem] text-[#e4405f]">Story reply</span>
          )}
          {message.forwarded && <span className="text-[0.688rem] text-muted">Forwarded</span>}
        </>
      }
    >
      <ServiceMessageText text={message.text || ""} highlight={highlight} mine={mine} />

      <MessageAttachments message={message} onAttachmentClick={onAttachmentClick} />
    </ServiceBubbleShell>
  );
}
