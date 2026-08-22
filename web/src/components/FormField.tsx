import type { ReactNode } from "react";

type FormFieldProps = {
  label: string;
  children: ReactNode;
  /** `inline` = label beside control (Extract/Format). `stacked` = label above (Import). */
  layout?: "inline" | "stacked";
  /** Optional control beside a stacked label (e.g. disclosure toggle). */
  trailing?: ReactNode;
};

/** Shared label + control layout for form screens. */
export default function FormField({
  label,
  children,
  layout = "inline",
  trailing,
}: FormFieldProps) {
  if (layout === "stacked") {
    return (
      <div className="mb-[1.1rem]">
        <div className="flex items-baseline gap-3">
          <label className="mb-1 block flex-1 text-[0.875rem] font-medium text-text">{label}</label>
          {trailing}
        </div>
        {children}
      </div>
    );
  }

  return (
    <div className="mb-3 flex items-center gap-3">
      <label className="w-[140px] shrink-0 text-[0.875rem] font-medium text-text">{label}</label>
      <div className="flex-1">{children}</div>
    </div>
  );
}
