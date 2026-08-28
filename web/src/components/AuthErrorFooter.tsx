/** The vault's messages start lowercase; a line of UI text should not. */
function sentenceCase(text: string): string {
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/**
 * Error line for an auth form. Always occupies space (transparent when empty)
 * so the surrounding form does not shift when a message appears. Callers place
 * it above the primary action, inside the card's pinned footer, so a message
 * grows upward into the card's slack rather than out of the fixed frame.
 */
export default function AuthErrorFooter({ error }: { error: string }) {
  return (
    <div
      className="mb-2 min-h-8 text-[0.813rem] leading-[1.35]"
      style={{ color: error ? "var(--danger)" : "transparent" }}
      aria-live="polite"
    >
      {sentenceCase(error) || " "}
    </div>
  );
}
