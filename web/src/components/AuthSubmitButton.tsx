import Button from "./Button";

/**
 * Primary action of an auth card. Half the card wide and pushed to the right,
 * so the action sits under the right edge of the fields rather than spanning
 * them, and close under the last field it acts on rather than pinned to the
 * foot of the frame.
 */
export default function AuthSubmitButton({
  children,
  disabled,
  onClick,
  className,
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  /** Overrides placement when the button shares a row with something else. */
  className?: string;
}) {
  return (
    <Button
      variant="primary"
      isDisabled={disabled}
      onPress={onClick}
      className={className ?? "mt-5 w-1/2 self-end"}
      type="submit"
    >
      {children}
    </Button>
  );
}
