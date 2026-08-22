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
          {displayParticipants.map((p, i) =>
            p.contact_id ? (
              <button
                key={`${p.contact_id}-${p.label}-${i}`}
                type="button"
                onClick={() => onOpenContact?.(p.contact_id!)}
                title={`Open contact for ${p.label}`}
                className="cursor-pointer rounded-full border border-border bg-panel px-2 py-0.5 text-[0.75rem] text-accent"
              >
                {p.label}
              </button>
            ) : (
              <span
                key={`${p.label}-${i}`}
                className="rounded-full border border-border bg-elevated px-2 py-0.5 text-[0.75rem] text-muted"
              >
                {p.label}
              </span>
            ),
          )}
        </div>
      )}

      <div role="separator" aria-hidden className="my-3 h-px bg-border" />

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
