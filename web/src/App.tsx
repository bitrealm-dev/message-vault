import { useState } from "react";
import Extract from "./screens/Extract";
import Format from "./screens/Format";
import Push from "./screens/Push";
import Pull from "./screens/Pull";

const TABS = [
  { id: "extract", label: "Extract" },
  { id: "format", label: "Format" },
  { id: "push", label: "Vault Push" },
  { id: "pull", label: "Vault Pull" },
] as const;

type TabId = (typeof TABS)[number]["id"];

function App() {
  const [tab, setTab] = useState<TabId>("extract");

  return (
    <div style={{ fontFamily: "system-ui", minHeight: "100vh", background: "#fafafa" }}>
      <nav
        style={{
          display: "flex",
          gap: 0,
          borderBottom: "2px solid #e5e7eb",
          background: "#fff",
          padding: "0 1.5rem",
        }}
      >
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            style={{
              padding: "0.75rem 1.25rem",
              border: "none",
              background: "none",
              fontSize: "0.875rem",
              fontWeight: tab === t.id ? 600 : 400,
              color: tab === t.id ? "#2563eb" : "#6b7280",
              borderBottom: tab === t.id ? "2px solid #2563eb" : "2px solid transparent",
              marginBottom: "-2px",
              cursor: "pointer",
            }}
          >
            {t.label}
          </button>
        ))}
      </nav>
      {tab === "extract" && <Extract />}
      {tab === "format" && <Format />}
      {tab === "push" && <Push />}
      {tab === "pull" && <Pull />}
    </div>
  );
}

export default App;
