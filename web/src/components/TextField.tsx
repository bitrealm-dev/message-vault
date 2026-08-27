import type { InputHTMLAttributes, KeyboardEventHandler, ReactNode } from "react";
import {
  FieldError,
  Input,
  Label,
  TextField as RACTextField,
  type TextFieldProps as RACTextFieldProps,
  Text,
} from "react-aria-components";

/** Shared chrome for text inputs (settings, forms, PathPicker, etc.). */
export const textInputClassName =
  "box-border w-full rounded-xl border border-border bg-bg px-3 py-2.5 text-[0.875rem] text-text outline-none focus:border-accent disabled:opacity-50";

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
  /** Optional control beside the label (e.g. a status light). */
  labelEnd?: ReactNode;
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
  labelEnd,
  hint,
  inputClassName,
  className,
  ...props
}: TextFieldProps) {
  return (
    <RACTextField {...props} className={className}>
      {label ? (
        <div className="mb-1 flex items-center gap-2">
          <Label className="text-[0.875rem] font-medium text-text">{label}</Label>
          {labelEnd}
        </div>
      ) : null}
      <Input className={`${textInputClassName} ${inputClassName ?? ""}`} />
      {hint && (
        <Text slot="description" className="mt-1 block text-[0.75rem] text-muted">
          {hint}
        </Text>
      )}
      <FieldError className="mt-1 block text-[0.75rem] text-danger" />
    </RACTextField>
  );
}
