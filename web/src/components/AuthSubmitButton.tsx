import Button from "./Button";

export default function AuthSubmitButton({
  children,
  disabled,
  onClick,
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick?: () => void;
}) {
  return (
    <Button variant="primary" isDisabled={disabled} onPress={onClick} className="mt-6 w-full" type="submit">
      {children}
    </Button>
  );
}
