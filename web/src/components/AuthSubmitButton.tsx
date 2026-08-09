import type { ButtonHTMLAttributes } from "react";
import Button from "./Button";

/** Full-width primary CTA for auth screens. */
export default function AuthSubmitButton({
  children,
  style,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <Button
      variant="primary"
      {...rest}
      style={{
        display: "block",
        width: "100%",
        marginTop: "1rem",
        padding: "0.75rem 1rem",
        fontSize: "1rem",
        ...style,
      }}
    >
      {children}
    </Button>
  );
}
