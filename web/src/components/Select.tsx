import {
  Select as RACSelect,
  SelectValue,
  Button,
  ListBox,
  ListBoxItem,
  Popover,
  Label,
  type SelectProps,
} from "react-aria-components";

/** Shared className render prop for every ListBoxItem inside a Select. */
export function selectItemClassName({
  isFocused,
  isSelected,
}: {
  isFocused: boolean;
  isSelected: boolean;
}): string {
  return (
    "cursor-pointer rounded px-2 py-1 text-[0.875rem] " +
    (isFocused ? "bg-hover" : "") +
    " " +
    (isSelected ? "bg-accent text-sent-text" : "text-text")
  );
}

/**
 * Shared select wrapping React Aria's Select + Popover + ListBox.
 *
 * Value handling follows React Aria: `selectedKey` / `onSelectionChange`
 * (Key-based) instead of the native `value` / `onChange`.
 */
export default function Select<T extends object>({
  label,
  triggerClassName,
  popoverClassName,
  className,
  children,
  ...props
}: SelectProps<T> & {
  label?: string;
  triggerClassName?: string;
  popoverClassName?: string;
  className?: string;
}) {
  return (
    <RACSelect {...props} className={className}>
      {label && <Label className="mb-1 block text-[0.875rem] font-medium text-text">{label}</Label>}
      <Button className={`flex w-full items-center justify-between gap-2 rounded-xl border border-border bg-bg px-3 py-2.5 text-[0.875rem] text-text outline-none focus:border-accent ${triggerClassName ?? ""}`}>
        <SelectValue className="truncate" />
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" className="ml-2 shrink-0 text-muted">
          <path d="M2.5 3.5 5 6l2.5-2.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </Button>
      <Popover className={`z-[100] min-w-[var(--trigger-width)] rounded-md border border-border bg-popover p-1 shadow-md outline-none ${popoverClassName ?? ""}`}>
        <ListBox className="max-h-72 overflow-auto outline-none">{children}</ListBox>
      </Popover>
    </RACSelect>
  );
}

export { ListBoxItem };
