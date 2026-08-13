import { Button as RACButton } from "react-aria-components";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

const VARIANT: Record<ButtonVariant, string> = {
  primary: "bg-accent text-sent-text border-accent font-bold hover:brightness-90",
  secondary: "bg-elevated text-text border-border font-semibold hover:bg-hover",
  danger: "bg-danger-soft-bg text-danger border-danger-soft-border font-semibold hover:brightness-95",
  ghost: "bg-transparent text-accent border-transparent font-medium hover:bg-hover",
};

export default function Button({
  variant = "secondary",
  children,
  disabled,
  title,
  style,
  className,
  ...rest
}: Omit<React.ComponentProps<typeof RACButton>, "className"> & {
  variant?: ButtonVariant;
  className?: string;
  disabled?: boolean;
  onClick?: React.MouseEventHandler<HTMLButtonElement>;
  title?: string;
}) {
  return (
    <RACButton
      {...rest}
      ref={(el) => {
        // The menu library strips `title`. Set it on the element so hover text still works.
        if (el && el.title !== title) el.title = title ?? "";
      }}
      isDisabled={disabled}
      style={style}
      className={`
        box-border inline-flex cursor-pointer select-none items-center justify-center
        rounded-md px-4 py-2 text-center text-[0.875rem] leading-[1.25]
        border border-solid font-[inherit]
        outline-none
        focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1
        disabled:cursor-not-allowed disabled:brightness-[0.72]
        ${VARIANT[variant]}
        ${className ?? ""}
      `}
    >
      {children}
    </RACButton>
  );
}
