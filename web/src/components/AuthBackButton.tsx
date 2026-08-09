import type { CSSProperties } from "react";

/** Fixed lower-left back control for auth screens (icon + plain label). */
export default function AuthBackButton({
  label,
  onClick,
}: {
  label: string;
  onClick: () => void;
}) {
  return (
    <button type="button" onClick={onClick} style={buttonStyle}>
      <BackArrowIcon />
      <span>{label}</span>
    </button>
  );
}

function BackArrowIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M19 12H5" />
      <path d="M12 19l-7-7 7-7" />
    </svg>
  );
}

const buttonStyle: CSSProperties = {
  all: "unset",
  boxSizing: "border-box",
  position: "fixed",
  left: "1.25rem",
  bottom: "1.25rem",
  zIndex: 10,
  display: "inline-flex",
  alignItems: "center",
  gap: "0.4rem",
  fontFamily: "inherit",
  fontSize: "0.875rem",
  fontWeight: 500,
  color: "var(--muted)",
  cursor: "pointer",
  textDecoration: "none",
};
