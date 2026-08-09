interface ErrorBannerProps {
  errors: string[];
  onDismiss: () => void;
}

export default function ErrorBanner({ errors, onDismiss }: ErrorBannerProps) {
  if (errors.length === 0) return null;

  return (
    <div
      style={{
        background: "var(--danger-soft-bg)",
        borderBottom: "2px solid var(--danger-soft-border)",
        padding: "0.75rem 1.5rem",
        display: "flex",
        alignItems: "flex-start",
        gap: "0.75rem",
      }}
    >
      <div style={{ flex: 1 }}>
        {errors.map((msg, i) => (
          <div
            key={i}
            style={{
              fontSize: "0.875rem",
              color: "var(--danger)",
              lineHeight: 1.5,
            }}
          >
            {msg}
          </div>
        ))}
      </div>
      <button
        onClick={onDismiss}
        style={{
          background: "none",
          border: "none",
          fontSize: "1.25rem",
          color: "var(--danger)",
          cursor: "pointer",
          padding: "0 0.25rem",
          lineHeight: 1,
        }}
        title="Dismiss"
      >
        ×
      </button>
    </div>
  );
}
