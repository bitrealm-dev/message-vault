import type { ReactNode, SVGProps } from "react";

type IconProps = {
  size?: number;
  className?: string;
} & Omit<SVGProps<SVGSVGElement>, "width" | "height" | "children">;

function IconShell({
  size = 13,
  className,
  children,
  ...rest
}: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className ?? "shrink-0"}
      {...rest}
    >
      {children}
    </svg>
  );
}

/** Edit pencil — diagonal outline with tip and ferrule line. */
export function PencilIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
      <path d="m15 5 4 4" />
    </IconShell>
  );
}

/** Delete / trash — lid with handle, can body, two inner lines. */
export function TrashIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </IconShell>
  );
}

/** Plus — horizontal and vertical stroke. */
export function PlusIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </IconShell>
  );
}

/** Chevron pointing right; rotate 90° when a section is open. */
export function ChevronRightIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <path d="m9 6 6 6-6 6" />
    </IconShell>
  );
}

/** Three dots for a row menu. */
export function EllipsisIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
      <circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none" />
    </IconShell>
  );
}

/** Two people — a contact group. */
export function PeopleGroupIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <circle cx="9" cy="8" r="3" />
      <path d="M3 19c0-2.8 2.7-5 6-5s6 2.2 6 5" />
      <circle cx="17" cy="9" r="2.4" />
      <path d="M16 14.2c2.4.4 4 2.2 4 4.8" />
    </IconShell>
  );
}

/** Price-tag shape — a thread tag. */
export function TagIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <path d="M20.6 13.4 12 22l-8.6-8.6a2 2 0 0 1 0-2.8L11.2 3H21v9.8a2 2 0 0 1-.4 1.6Z" />
      <circle cx="16.5" cy="7.5" r="1.2" />
    </IconShell>
  );
}

/** One person — contacts with no group. */
export function PersonIcon({ size, className, ...rest }: IconProps) {
  return (
    <IconShell size={size} className={className} {...rest}>
      <circle cx="12" cy="8" r="3" />
      <path d="M5 20c0-3.3 3.1-6 7-6s7 2.7 7 6" />
    </IconShell>
  );
}
