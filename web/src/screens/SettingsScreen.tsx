import { useState } from "react";
import { AccountSettingsPanel } from "./settings/AccountSettingsPanel";
import { ProfileSettingsPanel } from "./settings/ProfileSettingsPanel";
import { StorageSection } from "./settings/StorageSection";
import { SystemSection } from "./settings/SystemSection";
import { AppearanceSection } from "./settings/AppearanceSection";

type SettingsTab = "account" | "profile" | "storage" | "system" | "appearance";

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "account", label: "Account" },
  { id: "profile", label: "Profile" },
  { id: "storage", label: "Storage" },
  { id: "system", label: "System" },
  { id: "appearance", label: "Appearance" },
];

export default function SettingsScreen() {
  const [tab, setTab] = useState<SettingsTab>("account");

  return (
    <div style={{ padding: "1.5rem", maxWidth: "820px", color: "var(--text)" }}>
      <header style={{ marginBottom: "1.5rem" }}>
        <h2 style={{ margin: 0, color: "var(--text)" }}>Settings</h2>
        <p style={{ margin: "0.35rem 0 0", fontSize: "0.875rem", color: "var(--muted)" }}>
          Manage your account, profile, storage, system, and appearance.
        </p>
        <nav
          aria-label="Settings sections"
          style={{
            display: "flex",
            gap: "0.25rem",
            marginTop: "1.25rem",
            borderBottom: "1px solid var(--border)",
          }}
        >
          {TABS.map((t) => {
            const active = tab === t.id;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setTab(t.id)}
                style={{
                  position: "relative",
                  padding: "0.5rem 0.75rem",
                  fontSize: "0.813rem",
                  fontWeight: 500,
                  color: active ? "var(--text)" : "var(--muted)",
                  background: "transparent",
                  border: "none",
                  cursor: "pointer",
                  marginBottom: "-1px",
                }}
              >
                {t.label}
                {active && (
                  <span
                    aria-hidden
                    style={{
                      position: "absolute",
                      left: "0.5rem",
                      right: "0.5rem",
                      bottom: 0,
                      height: "2px",
                      borderRadius: "999px",
                      background: "var(--accent)",
                    }}
                  />
                )}
              </button>
            );
          })}
        </nav>
      </header>

      {tab === "account" && <AccountSettingsPanel />}
      {tab === "profile" && <ProfileSettingsPanel />}
      {tab === "storage" && <StorageSection />}
      {tab === "system" && <SystemSection />}
      {tab === "appearance" && <AppearanceSection />}
    </div>
  );
}
