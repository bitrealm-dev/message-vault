import { formatMonthYear } from "../../lib/formatDate";
import type { Conversation } from "../../lib/types";
import YearChipBar from "./YearChipBar";

export default function ConversationHeader({
  conversation,
  displayParticipants,
  participantsOpen,
  onToggleParticipants,
  sourceLabel,
  years,
  activeYear,
  onSelectAllYears,
  onSelectYear,
  onOpenContact,
  onShowSources,
}: {
  conversation: Conversation;
  displayParticipants: { label: string; contact_id?: string | null }[];
  participantsOpen: boolean;
  onToggleParticipants: () => void;
  sourceLabel: string;
  years: number[];
  activeYear: number | null;
  onSelectAllYears: () => void;
  onSelectYear: (year: number) => void;
  onOpenContact?: (contactId: string) => void;
  onShowSources: () => void;
}) {
  return (
    <div className="border-b border-border bg-elevated px-6 py-3">
      <button
        type="button"
        aria-expanded={participantsOpen}
        onClick={onToggleParticipants}
        disabled={displayParticipants.length === 0}
        className={`m-0 flex w-full items-center gap-2 border-none bg-transparent p-0 text-left text-[1rem] font-semibold text-text ${
          displayParticipants.length > 0 ? "cursor-pointer" : "cursor-default"
        }`}
      >
        {displayParticipants.length > 0 && (
          <span
            aria-hidden
            className={`inline-block shrink-0 text-[0.688rem] font-semibold text-muted transition-transform duration-150 ${
              participantsOpen ? "rotate-90" : ""
            }`}
          >
            ▶
          </span>
        )}
        <span className="min-w-0">
          {conversation.label ||
            (conversation.is_group
              ? `${conversation.participants.length} participants`
              : conversation.participants[0]?.name || conversation.participants[0]?.handle)}
        </span>
      </button>

      {participantsOpen && displayParticipants.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {(() => {
            const seen = new Map<string, number>();
            return displayParticipants.map((p) => {
              const contactId = p.contact_id;
              const base = contactId ? `c:${contactId}:${p.label}` : `l:${p.label}`;
              const n = seen.get(base) ?? 0;
              seen.set(base, n + 1);
              const key = n === 0 ? base : `${base}#${n}`;
              if (contactId) {
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => onOpenContact?.(contactId)}
                    title={`Open contact for ${p.label}`}
                    className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
                  >
                    {p.label}
                  </button>
                );
              }
              return (
                <span
                  key={key}
                  className="rounded-full border border-border bg-elevated px-2 py-0.5 text-[0.75rem] text-muted"
                >
                  {p.label}
                </span>
              );
            });
          })()}
        </div>
      )}

      <hr className="my-3 border-0 border-t border-border" />

      <div className="flex flex-wrap gap-4 text-[0.75rem] text-muted">
        <span>{sourceLabel}</span>
        {conversation.date_range_start && conversation.date_range_end && (
          <span>
            {formatMonthYear(conversation.date_range_start)} –{" "}
            {formatMonthYear(conversation.date_range_end)}
          </span>
        )}
        <span>{conversation.message_count} messages</span>
        <button
          type="button"
          onClick={onShowSources}
          className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
        >
          Sources
        </button>
      </div>

      <YearChipBar
        years={years}
        activeYear={activeYear}
        onSelectAll={onSelectAllYears}
        onSelectYear={onSelectYear}
      />
    </div>
  );
}
