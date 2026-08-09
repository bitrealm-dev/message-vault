import type { ButtonHTMLAttributes, CSSProperties } from "react";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

const VARIANT: Record<ButtonVariant, CSSProperties> = {
  primary: {
    backgroundColor: "var(--accent, #5ea1ff)",
    color: "#ffffff",
    border: "1px solid var(--accent, #5ea1ff)",
    fontWeight: 700,
  },
  secondary: {
    backgroundColor: "var(--elevated)",
    color: "var(--text)",
    border: "1px solid var(--border)",
    fontWeight: 600,
  },
  danger: {
    backgroundColor: "var(--danger-soft-bg)",
    color: "var(--danger)",
    border: "1px solid var(--danger-soft-border)",
    fontWeight: 600,
  },
  ghost: {
    backgroundColor: "transparent",
    color: "var(--accent)",
    border: "1px solid transparent",
    fontWeight: 500,
  },
};

/**
 * Theme-aware button that resets native chrome (`all: unset`).
 * Needed on WebKit/Tauri where system button backgrounds ignore CSS
 * while text color still applies (unreadable contrast).
 */
export default function Button({
  variant = "secondary",
  children,
  style,
  disabled,
  type = "button",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  const merged: CSSProperties = {
    all: "unset",
    boxSizing: "border-box",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    fontFamily: "inherit",
    fontSize: "0.875rem",
    lineHeight: 1.25,
    padding: "0.5rem 1rem",
    borderRadius: "6px",
    textAlign: "center",
    cursor: disabled ? "not-allowed" : "pointer",
    // Prefer brightness over opacity so labels stay readable when disabled.
    filter: disabled && variant !== "ghost" ? "brightness(0.72)" : "none",
    ...VARIANT[variant],
    ...style,
  };

  return (
    <button type={type} {...rest} disabled={disabled} style={merged}>
      {children}
    </button>
  );
}
