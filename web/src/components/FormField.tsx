import {
  Children,
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useId,
} from "react";

type FormFieldProps = {
  label: string;
  children: ReactNode;
  /** `inline` = label beside control (Export). `stacked` = label above (Import). */
  layout?: "inline" | "stacked";
  /** Optional control beside a stacked label (e.g. disclosure toggle). */
  trailing?: ReactNode;
  /** Red asterisk. Use when the field has no default and must be filled. */
  required?: boolean;
  /** Appends (Optional). Use when the field may stay empty and has no chosen default. */
  optional?: boolean;
};

function FieldLabelText({
  label,
  required,
  optional,
}: {
  label: string;
  required: boolean;
  optional: boolean;
}): ReactNode {
  return (
    <>
      {label}
      {required ? (
        <span className="text-danger" aria-hidden>
          {" *"}
        </span>
      ) : null}
      {!required && optional ? " (Optional)" : null}
    </>
  );
}

function withControlId(children: ReactNode, id: string): ReactNode {
  const list = Children.toArray(children);
  let assigned = false;
  return list.map((child) => {
    if (!assigned && isValidElement(child)) {
      assigned = true;
      const el = child as ReactElement<{ id?: string }>;
      return cloneElement(el, { id: el.props.id ?? id });
    }
    return child;
  });
}

/** Shared label + control layout for form screens. */
export default function FormField({
  label,
  children,
  layout = "inline",
  trailing,
  required = false,
  optional = false,
}: FormFieldProps) {
  const id = useId();
  const control = withControlId(children, id);
  const labelText = <FieldLabelText label={label} required={required} optional={optional} />;

  if (layout === "stacked") {
    return (
      <div className="mb-[1.1rem]">
        <div className="mb-1 flex items-baseline gap-3">
          <label htmlFor={id} className="block flex-1 text-[0.875rem] font-medium text-text">
            {labelText}
          </label>
          {trailing}
        </div>
        {control}
      </div>
    );
  }

  return (
    <div className="mb-3 flex items-center gap-3">
      <label htmlFor={id} className="w-[140px] shrink-0 text-[0.875rem] font-medium text-text">
        {labelText}
      </label>
      <div className="flex-1">{control}</div>
    </div>
  );
}
