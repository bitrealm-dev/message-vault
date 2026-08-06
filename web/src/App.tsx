import { useState, useCallback } from "react";
import Home from "./screens/Home";
import Extract from "./screens/Extract";
import Format from "./screens/Format";
import Push from "./screens/Push";
import Pull from "./screens/Pull";
import Settings from "./screens/Settings";
import ErrorBanner from "./components/ErrorBanner";
import { getErrors, clearErrors } from "./lib/tauri";

const TABS = [
  { id: "home", label: "Home" },
  { id: "extract", label: "Extract" },
  { id: "format", label: "Format" },
  { id: "push", label: "Vault Push" },
  { id: "pull", label: "Vault Pull" },
  { id: "settings", label: "Settings" },
] as const;

type TabId = (typeof TABS)[number]["id"];

function App() {
  const [tab, setTab] = useState<TabId>("home");
  const [errors, setErrors] = useState<string[]>([]);

  // Check for startup errors (e.g. corrupt export.ini)
  useState(() => {
    getErrors().then((errs) => {
      if (errs.length > 0) setErrors(errs);
    });
  });

  const handleNavigate = useCallback((target: string) => {
    if (TABS.some((t) => t.id === target)) {
      setTab(target as TabId);
    }
  }, []);

  const handleDismissErrors = useCallback(async () => {
    setErrors([]);
    await clearErrors();
  }, []);

  const handleJobError = useCallback((message: string) => {
    setErrors([message]);
  }, []);

  return (
    <div style={{ fontFamily: "system-ui", minHeight: "100vh", background: "#fafafa" }}>
      <ErrorBanner errors={errors} onDismiss={handleDismissErrors} />
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
      {tab === "home" && <Home onNavigate={handleNavigate} />}
      {tab === "extract" && <Extract onError={handleJobError} />}
      {tab === "format" && <Format onError={handleJobError} />}
      {tab === "push" && <Push onError={handleJobError} />}
      {tab === "pull" && <Pull onError={handleJobError} />}
      {tab === "settings" && <Settings />}
    </div>
  );
}

export default App;
