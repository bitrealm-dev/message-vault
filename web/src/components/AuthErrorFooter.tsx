/**
 * Error line under an auth form. Always occupies space (transparent when empty)
 * so the surrounding form does not shift when a message appears.
 */
export default function AuthErrorFooter({ error }: { error: string }) {
  return (
    <div
      className="mt-5 min-h-10 text-[0.813rem] leading-[1.35]"
      style={{ color: error ? "var(--danger)" : "transparent" }}
      aria-live="polite"
    >
      {error || "\u00a0"}
    </div>
  );
}
