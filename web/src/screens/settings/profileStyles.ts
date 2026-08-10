export interface AccountProfile {
  account_id: string;
  username: string;
  preferred_name: string | null;
  phones: string[];
  emails: string[];
  is_demo?: boolean;
  read_only?: boolean;
}

export const inputClassName = "w-full box-border px-3 py-2 text-[0.875rem] rounded border border-border bg-elevated text-text focus:outline-none focus:border-accent disabled:opacity-50";
export const sectionTitleClass = "text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted mb-2";
export const dangerButtonClass = "text-[0.813rem] text-danger bg-transparent border border-[var(--danger-soft-border)] rounded px-3 py-1.5 cursor-pointer hover:bg-[var(--danger-soft-bg)]";
