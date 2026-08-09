"use client";

/** Hoverable group-header participant chip (highlight + trailing chevron). */
export function GroupParticipantChip({
  label,
  onClick,
}: {
  label: string;
  onClick: (anchor: DOMRect) => void;
}) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick(e.currentTarget.getBoundingClientRect());
      }}
      className="group/chip inline-flex max-w-full items-center gap-0.5 whitespace-nowrap rounded-md px-1.5 py-0.5 text-inherit transition-colors hover:bg-hover hover:text-text"
    >
      <span className="truncate">{label}</span>
      {/* Fixed-width slot so hover never changes chip size / reflows the name wrap. */}
      <span
        aria-hidden
        className="inline-flex w-2.5 shrink-0 items-center justify-center opacity-0 transition-opacity group-hover/chip:opacity-70"
      >
        ›
      </span>
    </button>
  );
}

export function GroupParticipantNameSep() {
  return (
    <span className="px-1.5 font-normal opacity-70" aria-hidden>
      ·
    </span>
  );
}
