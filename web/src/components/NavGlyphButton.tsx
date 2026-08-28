import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";

/**
 * 24px (`size-6`) round hit target for left-nav plus, ellipsis, and delete.
 * Trailing column width in `navSectionLayout` must stay 1.5rem to match.
 */
export default function NavGlyphButton({
  children,
  active = false,
  danger = false,
  className = "",
  ref,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode;
  /** Keep the hover circle on while a row menu is open. */
  active?: boolean;
  /** Trash control: keep the circle, use the danger color on the glyph. */
  danger?: boolean;
  /** Menus that open from this button return focus here when they close. */
  ref?: Ref<HTMLButtonElement>;
}) {
  const hoverText = danger
    ? "hover:text-danger focus-visible:text-danger"
    : "hover:text-text focus-visible:text-text";
  return (
    <button
      type="button"
      ref={ref}
      className={`box-border flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-full border-none bg-transparent p-0 text-muted hover:bg-hover focus-visible:bg-hover disabled:cursor-default disabled:opacity-40 ${hoverText} ${
        active ? "bg-hover text-text" : ""
      } ${className}`.trim()}
      {...rest}
    >
      {children}
    </button>
  );
}
