import { selectItemClassName } from "../Select";

/** Compact text input for the filter grid (8px vertical to keep rows dense). */
export const inputClass =
  "box-border w-full rounded-md border border-border bg-bg px-2 py-1 text-[0.813rem] text-text outline-none focus:border-accent";

/** Uppercase micro-label above each filter field. */
export const labelClass =
  "mb-1 block text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted";

export const dateGroupClass =
  "box-border flex w-full min-w-0 items-center overflow-hidden rounded-md border border-border bg-bg px-2 py-1 focus-within:border-accent";

/** Select triggers in this panel — slightly squarer than the shared Select default. */
export const selectTriggerClass = "!rounded-md !bg-bg";

/** Compact Select trigger matching filter field padding/size. */
export const compactFieldTriggerClass = `!box-border !min-w-0 !px-2 !py-1 !text-[0.813rem] ${selectTriggerClass}`;

/** Single-column stack used for every contacts filter block. */
export const contactStackClass = "flex min-w-0 flex-col gap-3";

/** Menu rows sized to match the compact field text (0.813rem), not the shared 0.875rem Select. */
export function compactSelectItemClassName(state: {
  isFocused: boolean;
  isSelected: boolean;
}): string {
  return selectItemClassName(state).replace("text-[0.875rem]", "text-[0.813rem]");
}
