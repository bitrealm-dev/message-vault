interface Step {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

/**
 * Ordered list of import steps. Each step is an <li>; the active step carries
 * aria-current="step" so assistive tech announces where the job is.
 */
export default function StepProgress({ steps }: { steps: Step[] }) {
  return (
    <ol className="mt-6">
      {steps.map((step, i) => (
        <li
          key={i}
          aria-current={step.status === "active" ? "step" : undefined}
          className="mb-3 flex items-start gap-3"
        >
          <span
            className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[0.75rem] font-semibold ${
              step.status === "done" ? "bg-ok text-sent-text" :
              step.status === "active" ? "bg-accent text-sent-text" :
              step.status === "error" ? "bg-danger text-sent-text" : "bg-border text-muted"
            }`}
          >
            {step.status === "done" ? "✓" : step.status === "error" ? "!" : i + 1}
          </span>
          <div>
            <div
              className={`text-[0.875rem] ${
                step.status === "active" ? "font-semibold text-text" :
                step.status === "pending" ? "text-muted" : "text-text"
              }`}
            >
              {step.label}
            </div>
            {step.detail && <div className="mt-0.5 text-[0.75rem] text-muted">{step.detail}</div>}
          </div>
        </li>
      ))}
    </ol>
  );
}
