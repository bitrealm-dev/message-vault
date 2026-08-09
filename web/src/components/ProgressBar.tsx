interface ProgressBarProps {
  log: string[];
  running: boolean;
}

const INDETERMINATE_KEYFRAMES = `
@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
`;

export default function ProgressBar({ log, running }: ProgressBarProps) {
  return (
    <div>
      <style>{INDETERMINATE_KEYFRAMES}</style>
      {running && (
        <div style={{ marginBottom: "0.5rem" }}>
          <div
            style={{
              height: "8px",
              background: "var(--border)",
              borderRadius: "4px",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: "100%",
                width: "100%",
                background: "var(--accent)",
                animation: "indeterminate 1.5s ease-in-out infinite",
              }}
            />
          </div>
        </div>
      )}
      {log.length > 0 && (
        <pre
          style={{
            maxHeight: "300px",
            overflow: "auto",
            fontSize: "0.75rem",
            background: "var(--hover)",
            padding: "0.5rem",
            borderRadius: "4px",
            margin: 0,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
          }}
        >
          {log.map((line, i) => (
            <div key={i}>{line}</div>
          ))}
        </pre>
      )}
    </div>
  );
}
