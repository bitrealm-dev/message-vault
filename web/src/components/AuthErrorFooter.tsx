/** The vault's messages start lowercase; a line of UI text should not. */
function sentenceCase(text: string): string {
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/**
 * Error line for an auth form. Always occupies the same two lines, transparent
 * when empty, so the surrounding form does not shift when a message appears —
 * and a long one scrolls inside that band instead of growing the footer. The
 * card is a fixed height, so an unbounded message would push the primary
 * action out through the bottom of the frame and onto whatever sits below it.
 * The right padding is the width of a scrollbar, so a message that scrolls
 * does not run underneath its own thumb.
 */
export default function AuthErrorFooter({ error }: { error: string }) {
  return (
    <div
      className="mb-2 h-9 overflow-y-auto pr-2 text-[0.813rem] leading-[1.35]"
      style={{ color: error ? "var(--danger)" : "transparent" }}
      aria-live="polite"
    >
      {sentenceCase(error) || " "}
    </div>
  );
}
