import { cloneElement, isValidElement, type ReactElement, type ReactNode, useId } from "react";

type FormFieldProps = {
  label: string;
  children: ReactNode;
  /** `inline` = label beside control (Extract/Format). `stacked` = label above (Import). */
  layout?: "inline" | "stacked";
  /** Optional control beside a stacked label (e.g. disclosure toggle). */
  trailing?: ReactNode;
};

function withControlId(children: ReactNode, id: string): ReactNode {
  if (!isValidElement(children)) {
    return children;
  }
  const el = children as ReactElement<{ id?: string }>;
  return cloneElement(el, { id: el.props.id ?? id });
}

/** Shared label + control layout for form screens. */
export default function FormField({
  label,
  children,
  layout = "inline",
  trailing,
}: FormFieldProps) {
  const id = useId();
  const control = withControlId(children, id);

  if (layout === "stacked") {
    return (
      <div className="mb-[1.1rem]">
        <div className="mb-1 flex items-baseline gap-3">
          <label htmlFor={id} className="block flex-1 text-[0.875rem] font-medium text-text">
            {label}
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
        {label}
      </label>
      <div className="flex-1">{control}</div>
    </div>
  );
}
