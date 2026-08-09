import { useAuth } from "../lib/auth";

interface QuickAction {
  id: string;
  label: string;
  description: string;
  color: string;
}

const ACTIONS: QuickAction[] = [
  {
    id: "extract",
    label: "Extract Messages",
    description: "Pull conversations from a phone backup and export them as JSONL",
    color: "#2563eb",
  },
  {
    id: "format",
    label: "Format Conversion",
    description: "Convert an existing extract to EML, MBOX, CSV, or XML",
    color: "#7c3aed",
  },
  {
    id: "push",
    label: "Vault Push",
    description: "Import messages into a Message Vault server",
    color: "#059669",
  },
  {
    id: "pull",
    label: "Vault Pull",
    description: "Export messages from a Message Vault server",
    color: "#d97706",
  },
];

export default function Home({ onNavigate }: { onNavigate: (tab: string) => void }) {
  const { isAuthenticated, serverUrl } = useAuth();

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 0.5rem 0", fontSize: "1.5rem" }}>Message Vault</h2>
      <p style={{ margin: "0 0 2rem 0", color: "#6b7280", fontSize: "0.875rem" }}>
        Extract, convert, and manage message backups
      </p>

      <div
        style={{
          background: isAuthenticated ? "#f0fdf4" : "#fefce8",
          border: isAuthenticated ? "1px solid #bbf7d0" : "1px solid #fef08a",
          borderRadius: "8px",
          padding: "0.75rem 1rem",
          marginBottom: "2rem",
          fontSize: "0.875rem",
        }}
      >
        <span style={{ fontWeight: 600, color: isAuthenticated ? "#166534" : "#854d0e" }}>
          Vault:{" "}
        </span>
        {isAuthenticated ? (
          <span style={{ color: "#166534" }}>
            Signed in{serverUrl ? ` at ${serverUrl}` : ""}
          </span>
        ) : (
          <span style={{ color: "#854d0e" }}>
            Sign in to use vault import and browse
          </span>
        )}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {ACTIONS.map((action) => (
          <button
            key={action.id}
            onClick={() => onNavigate(action.id)}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "1rem",
              padding: "1rem 1.25rem",
              border: "1px solid #e5e7eb",
              borderRadius: "8px",
              background: "#fff",
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            <span
              style={{
                width: "8px",
                height: "40px",
                borderRadius: "4px",
                background: action.color,
                flexShrink: 0,
              }}
            />
            <span>
              <span style={{ display: "block", fontWeight: 600, color: "#111827" }}>
                {action.label}
              </span>
              <span style={{ display: "block", fontSize: "0.813rem", color: "#6b7280" }}>
                {action.description}
              </span>
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
