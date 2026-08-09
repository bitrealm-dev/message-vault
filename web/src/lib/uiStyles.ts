import type { CSSProperties } from "react";

/** Shared theme-aware styles using CSS vars from theme.css. */

export const pageCenter: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "100vh",
  background: "var(--bg)",
  color: "var(--text)",
  fontFamily: "system-ui",
};

export const authCard: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  background: "var(--panel)",
  color: "var(--text)",
  padding: "2rem",
  borderRadius: "8px",
  width: "400px",
  height: "32rem",
  maxWidth: "100%",
  boxSizing: "border-box",
  overflow: "hidden",
  border: "1px solid var(--border)",
  boxShadow: "0 8px 24px var(--scrim)",
};

export const authTitle: CSSProperties = {
  margin: "0 0 1.5rem",
  fontSize: "1.5rem",
  textAlign: "center",
  color: "var(--text)",
};

export const authLabel: CSSProperties = {
  fontSize: "0.875rem",
  fontWeight: 500,
  display: "block",
  marginBottom: "0.25rem",
  color: "var(--text)",
};

export const authInput: CSSProperties = {
  width: "100%",
  padding: "0.5rem",
  fontSize: "0.875rem",
  border: "1px solid var(--border)",
  borderRadius: "4px",
  boxSizing: "border-box",
  background: "var(--elevated)",
  color: "var(--text)",
};

/** Theme tokens for bare `<input>` / `<select>` (overrides fragile browser defaults). */
export const controlInput: CSSProperties = {
  background: "var(--bg)",
  color: "var(--text)",
  border: "1px solid var(--border)",
  borderRadius: "4px",
  boxSizing: "border-box",
};

export const mutedText: CSSProperties = {
  color: "var(--muted)",
};

export const accentLink: CSSProperties = {
  display: "block",
  width: "100%",
  padding: "0.25rem",
  fontSize: "0.875rem",
  appearance: "none",
  WebkitAppearance: "none",
  background: "transparent",
  backgroundColor: "transparent",
  border: "none",
  color: "var(--accent)",
  textDecoration: "underline",
  cursor: "pointer",
  textAlign: "center",
};

export const mutedLink: CSSProperties = {
  display: "block",
  width: "100%",
  marginTop: "1rem",
  padding: "0.25rem",
  fontSize: "0.813rem",
  appearance: "none",
  WebkitAppearance: "none",
  background: "transparent",
  backgroundColor: "transparent",
  border: "none",
  color: "var(--muted)",
  cursor: "pointer",
  textAlign: "center",
};

export const dangerText: CSSProperties = {
  color: "var(--danger)",
};

export const divider: CSSProperties = {
  margin: "1.5rem 0",
  border: "none",
  borderTop: "1px solid var(--border)",
};

export const primaryButton: CSSProperties = {
  all: "unset",
  boxSizing: "border-box",
  display: "inline-block",
  fontFamily: "inherit",
  fontSize: "0.875rem",
  lineHeight: 1.25,
  padding: "0.5rem 1rem",
  borderRadius: "6px",
  textAlign: "center",
  cursor: "pointer",
  backgroundColor: "var(--accent, #5ea1ff)",
  color: "#ffffff",
  border: "1px solid var(--accent, #5ea1ff)",
  fontWeight: 700,
};

export const secondaryButton: CSSProperties = {
  all: "unset",
  boxSizing: "border-box",
  display: "inline-block",
  fontFamily: "inherit",
  fontSize: "0.875rem",
  lineHeight: 1.25,
  padding: "0.5rem 1rem",
  borderRadius: "6px",
  textAlign: "center",
  cursor: "pointer",
  backgroundColor: "var(--elevated)",
  color: "var(--text)",
  border: "1px solid var(--border)",
  fontWeight: 600,
};
