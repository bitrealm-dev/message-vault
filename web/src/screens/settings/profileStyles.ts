import type { CSSProperties } from "react";

export interface AccountProfile {
  account_id: string;
  username: string;
  preferred_name: string | null;
  phones: string[];
  emails: string[];
  is_demo?: boolean;
  read_only?: boolean;
}

export const inputStyle: CSSProperties = {
  width: "100%",
  padding: "0.35rem 0.5rem",
  fontSize: "0.875rem",
  border: "1px solid var(--border)",
  borderRadius: "4px",
  boxSizing: "border-box",
  // backgroundColor (not background) so select keeps the themed chevron.
  backgroundColor: "var(--elevated)",
  color: "var(--text)",
};

export const sectionTitle: CSSProperties = {
  fontSize: "0.875rem",
  color: "var(--muted)",
  margin: "0 0 0.5rem",
};

export const dangerButtonStyle: CSSProperties = {
  width: "10rem",
  flexShrink: 0,
  padding: "0.5rem 0.75rem",
  fontSize: "0.813rem",
};
