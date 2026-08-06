interface FormRowProps {
  label: string;
  children: React.ReactNode;
}

export default function FormRow({ label, children }: FormRowProps) {
  return (
    <div style={{ display: "flex", alignItems: "center", marginBottom: "0.75rem", gap: "0.75rem" }}>
      <label style={{ width: "140px", flexShrink: 0, fontWeight: 500, fontSize: "0.875rem" }}>
        {label}
      </label>
      <div style={{ flex: 1 }}>{children}</div>
    </div>
  );
}
