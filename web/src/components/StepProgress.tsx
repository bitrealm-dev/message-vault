interface Step {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function StepProgress({ steps }: { steps: Step[] }) {
  return (
    <div style={{ marginTop: "1.5rem" }}>
      {steps.map((step, i) => (
        <div key={i} style={{ display: "flex", gap: "0.75rem", marginBottom: "0.75rem", alignItems: "flex-start" }}>
          <div style={{
            width: "24px", height: "24px", borderRadius: "50%", flexShrink: 0,
            display: "flex", alignItems: "center", justifyContent: "center",
            fontSize: "0.75rem", fontWeight: 600,
            background:
              step.status === "done" ? "var(--ok)" :
              step.status === "active" ? "var(--accent)" :
              step.status === "error" ? "var(--danger)" : "var(--border)",
            color: step.status === "pending" ? "var(--muted)" : "var(--sent-text)",
          }}>
            {step.status === "done" ? "✓" : step.status === "error" ? "!" : i + 1}
          </div>
          <div>
            <div style={{
              fontSize: "0.875rem", fontWeight: step.status === "active" ? 600 : 400,
              color: step.status === "active" ? "var(--text)" : step.status === "pending" ? "var(--muted)" : "var(--text)",
            }}>
              {step.label}
            </div>
            {step.detail && <div style={{ fontSize: "0.75rem", color: "var(--muted)", marginTop: "2px" }}>{step.detail}</div>}
          </div>
        </div>
      ))}
    </div>
  );
}
