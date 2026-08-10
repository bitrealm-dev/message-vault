import Button from "./Button";

export default function AuthBackButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <Button variant="ghost" onPress={onClick} className="-ml-2 gap-1.5">
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M6.5 1.5 3 5l3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      {label}
    </Button>
  );
}
