import { dataCardBodyCellClass, dataCardHeaderCellClass } from "../DataCard";

export const thClass = dataCardHeaderCellClass;
export const tdClass = dataCardBodyCellClass;
export const tdCenterClass = tdClass;
export const tdRightClass = `${tdClass} text-right`;
export const thRightClass = `${thClass} text-right`;
export const linkClass =
  "border-none bg-transparent p-0 text-[0.813rem] font-semibold leading-snug text-accent underline decoration-accent/80 underline-offset-2 cursor-pointer outline-none hover:decoration-accent hover:opacity-90 focus-visible:rounded-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";
export const mutedClass = "text-[0.813rem] leading-snug text-muted";
export const iconBtnDangerClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !border-transparent !bg-transparent !p-0 !font-normal !leading-none !text-muted hover:!border-danger-soft-border hover:!bg-danger-soft-bg hover:!text-danger data-hovered:!border-danger-soft-border data-hovered:!bg-danger-soft-bg data-hovered:!text-danger data-pressed:!border-danger-soft-border data-pressed:!bg-danger-soft-bg data-pressed:!text-danger";
/** Trash: show on row hover; on keyboard, when the button itself is focus-visible.
 * Avoid row focus-within — table row focus after click would leave trash stuck on. */
export const rowActionsRevealClass =
  "opacity-100 [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover/handle-row:opacity-100 [@media(hover:hover)]:group-data-hovered/handle-row:opacity-100 [@media(hover:hover)]:has-[:focus-visible]:opacity-100";
