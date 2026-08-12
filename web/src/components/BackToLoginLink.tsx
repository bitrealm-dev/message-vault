/**
 * Return link for the offline tools reachable from the login screen.
 * Renders nothing when there is nowhere to go back to (e.g. inside the vault UI).
 */
export default function BackToLoginLink({ onBack }: { onBack?: () => void }) {
  if (!onBack) return null;

  return (
    <button
      onClick={onBack}
      className="mb-4 cursor-pointer border-none bg-none p-0 text-[0.875rem] text-accent"
    >
      ← Back to login
    </button>
  );
}
