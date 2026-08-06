interface ProgressBarProps {
  log: string[];
  running: boolean;
}

export default function ProgressBar({ log, running }: ProgressBarProps) {
  return (
    <div>
      {running && (
        <div style={{ marginBottom: "0.5rem" }}>
          <div
            style={{
              height: "8px",
              background: "#e5e7eb",
              borderRadius: "4px",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: "100%",
                width: "100%",
                background: "#3b82f6",
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
            background: "#f3f4f6",
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
