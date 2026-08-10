import {
  TextField as RACTextField,
  Input,
  Label,
  FieldError,
  Text,
  type TextFieldProps as RACTextFieldProps,
} from "react-aria-components";
import type { InputHTMLAttributes, KeyboardEventHandler } from "react";

/**
 * Shared text input wrapping React Aria's TextField + Input.
 *
 * `label` renders an internal Label; `hint` renders a description slot.
 * React Aria's TextField type omits some input-level props (placeholder,
 * onKeyDown, ...) even though it forwards them at runtime — re-declare them
 * so callers can pass them straight through.
 */
export interface TextFieldProps extends RACTextFieldProps {
  label?: string;
  hint?: string;
  inputClassName?: string;
  className?: string;
  placeholder?: string;
  autoComplete?: string;
  autoFocus?: boolean;
  type?: string;
  name?: string;
  inputMode?: InputHTMLAttributes<HTMLInputElement>["inputMode"];
  onKeyDown?: KeyboardEventHandler<HTMLInputElement>;
  maxLength?: number;
  minLength?: number;
  pattern?: string;
}

export default function TextField({
  label,
  hint,
  inputClassName,
  className,
  ...props
}: TextFieldProps) {
  return (
    <RACTextField {...props} className={className}>
      {label && <Label className="mb-1 block text-[0.875rem] font-medium text-text">{label}</Label>}
      <Input
        className={`box-border w-full rounded border border-border bg-elevated px-2 py-1.5 text-[0.875rem] text-text outline-none focus:border-accent disabled:opacity-50 ${inputClassName ?? ""}`}
      />
      {hint && <Text slot="description" className="mt-1 block text-[0.75rem] text-muted">{hint}</Text>}
      <FieldError className="mt-1 block text-[0.75rem] text-danger" />
    </RACTextField>
  );
}
