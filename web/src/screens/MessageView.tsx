import { useCallback, useEffect, useMemo, useState } from "react";
import AttachmentLightbox, { type LightboxItem } from "../components/AttachmentLightbox";
import SourcesPanel from "../components/SourcesPanel";
import { personDisplayLabel } from "../lib/nameAliases";
import type { Conversation, MessageAttachment } from "../lib/types";
import { useNameAliases } from "../lib/useNameAliases";
import ConversationHeader from "./message/ConversationHeader";
import MessageFindBar from "./message/MessageFindBar";
import MessageThread from "./message/MessageThread";
import {
  buildFooterLabel,
  conversationYears,
  displaySourceLabel,
  PAGE_SIZE,
  useConversationMessages,
} from "./message/useConversationMessages";

export default function MessageView({
  conversation,
  onOpenContact,
}: {
  conversation: Conversation;
  onOpenContact?: (contactId: string) => void;
}) {
  const {
    messages,
    total,
    offset,
    activeYear,
    findTerm,
    setFindTerm,
    activeMatch,
    setActiveMatch,
    loading,
    fetchConversationPage,
    selectAllYears,
    selectYear,
  } = useConversationMessages(conversation.id);

  const [lightboxItems, setLightboxItems] = useState<LightboxItem[] | null>(null);
  // Fallback source label from the first loaded message until the header has a better one.
  const sourceLabel = messages[0] ? displaySourceLabel(messages[0].source) : "unknown";
  const [lightboxIndex, setLightboxIndex] = useState(0);
  const [showSources, setShowSources] = useState(false);
  const [participantsOpen, setParticipantsOpen] = useState(true);

  const years = useMemo(
    () => conversationYears(conversation.date_range_start, conversation.date_range_end),
    [conversation.date_range_start, conversation.date_range_end],
  );

  // Open the image viewer at the clicked photo. Previous/next walks this page's images.
  const handleAttachmentClick = useCallback(
    (att: MessageAttachment, source: string) => {
      const images = messages.flatMap((m) =>
        (m.attachments || [])
          .filter((a) => a.sha256 && a.mime_type?.startsWith("image/"))
          .map((a) => ({ attachment: a, source: m.source })),
      );
      const idx = images.findIndex(
        (item) => item.attachment.sha256 === att.sha256 && item.source === source,
      );
      setLightboxItems(images.length > 0 ? images : [{ attachment: att, source }]);
      setLightboxIndex(idx >= 0 ? idx : 0);
    },
    [messages],
  );

  useEffect(() => {
    setParticipantsOpen(true);
  }, []);

  /** Prefer list-API participants; fall back to the loaded page's conversation header. */
  const useAliases = useNameAliases();
  const displayParticipants = useMemo(() => {
    if (conversation.participants.length > 0) {
      return conversation.participants.map((p) => ({
        label: personDisplayLabel(
          {
            preferredName: p.name,
            nameAlias: p.name_alias,
            handle: p.handle,
          },
          useAliases,
        ),
        contact_id: p.contact_id,
      }));
    }
    const fromMsg = messages[0]?.conversation.participants || [];
    return fromMsg.map((p) => ({
      label: personDisplayLabel(
        {
          preferredName: p.preferred_name,
          nameAlias: p.name_alias,
          handle: p.handle,
        },
        useAliases,
      ),
      contact_id: p.contact_id,
    }));
  }, [conversation.participants, messages, useAliases]);

  // Message ids on this page whose text contains the find-bar search.
  const matchIds = useMemo(() => {
    const t = findTerm.trim().toLowerCase();
    if (!t) return [];
    return messages.filter((m) => (m.text || "").toLowerCase().includes(t)).map((m) => m.id);
  }, [messages, findTerm]);

  // Scroll the current find match into view.
  useEffect(() => {
    if (!matchIds.length) return;
    const el = document.getElementById(`msg-${matchIds[activeMatch]}`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeMatch, matchIds]);

  const yearMode = activeYear !== null;
  const footerLabel = buildFooterLabel(activeYear, total, offset);

  return (
    <div className="flex h-full flex-col">
      <ConversationHeader
        conversation={conversation}
        displayParticipants={displayParticipants}
        participantsOpen={participantsOpen}
        onToggleParticipants={() => setParticipantsOpen((o) => !o)}
        sourceLabel={sourceLabel}
        years={years}
        activeYear={activeYear}
        onSelectAllYears={selectAllYears}
        onSelectYear={selectYear}
        onOpenContact={onOpenContact}
        onShowSources={() => setShowSources(true)}
      />

      <MessageFindBar
        findTerm={findTerm}
        onFindTermChange={(value) => {
          setFindTerm(value);
          setActiveMatch(0);
        }}
        matchCount={matchIds.length}
        activeMatch={activeMatch}
        yearMode={yearMode}
        onPrevMatch={() => setActiveMatch((a) => (a - 1 + matchIds.length) % matchIds.length)}
        onNextMatch={() => setActiveMatch((a) => (a + 1) % matchIds.length)}
      />

      <MessageThread
        messages={messages}
        loading={loading}
        findTerm={findTerm}
        matchIds={matchIds}
        activeMatch={activeMatch}
        yearMode={yearMode}
        footerLabel={footerLabel}
        offset={offset}
        total={total}
        onPrevPage={() => void fetchConversationPage(Math.max(0, offset - PAGE_SIZE))}
        onNextPage={() => void fetchConversationPage(offset + PAGE_SIZE)}
        onAttachmentClick={handleAttachmentClick}
      />

      {lightboxItems && (
        <AttachmentLightbox
          items={lightboxItems}
          currentIndex={lightboxIndex}
          onClose={() => setLightboxItems(null)}
          onPrev={() =>
            setLightboxIndex((i) => (i - 1 + lightboxItems.length) % lightboxItems.length)
          }
          onNext={() => setLightboxIndex((i) => (i + 1) % lightboxItems.length)}
        />
      )}

      {showSources && (
        <SourcesPanel conversationId={conversation.id} onClose={() => setShowSources(false)} />
      )}
    </div>
  );
}
