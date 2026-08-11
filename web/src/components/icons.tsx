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
