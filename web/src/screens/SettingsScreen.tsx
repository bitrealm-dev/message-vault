import { useState } from "react";
import { Tabs, TabList, Tab, TabPanel } from "react-aria-components";
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

// Keeps the original look: text-colored when active, muted otherwise, with the
// active underline rendered as an absolutely-positioned bar sitting on the
// TabList's bottom border.
function tabClassName({ isSelected }: { isSelected: boolean }) {
  return `relative -mb-px cursor-pointer border-none bg-transparent px-3 py-2 text-[0.813rem] font-medium outline-none focus-visible:ring-2 focus-visible:ring-accent ${
    isSelected ? "text-text" : "text-muted hover:text-text"
  }`;
}

export default function SettingsScreen() {
  const [tab, setTab] = useState<SettingsTab>("account");

  return (
    <div style={{ padding: "1.5rem", maxWidth: "820px", color: "var(--text)" }}>
      <header style={{ marginBottom: 0 }}>
        <h2 style={{ margin: 0, color: "var(--text)" }}>Settings</h2>
        <p style={{ margin: "0.35rem 0 0", fontSize: "0.875rem", color: "var(--muted)" }}>
          Manage your account, profile, storage, system, and appearance.
        </p>
      </header>

      <Tabs selectedKey={tab} onSelectionChange={(key) => setTab(key as SettingsTab)}>
        <TabList
          aria-label="Settings sections"
          className="mt-5 flex gap-1 border-b border-border"
        >
          {TABS.map((t) => (
            <Tab key={t.id} id={t.id} className={tabClassName}>
              {({ isSelected }) => (
                <>
                  {t.label}
                  {isSelected && (
                    <span
                      aria-hidden
                      className="absolute bottom-0 left-2 right-2 h-[2px] rounded-full bg-accent"
                    />
                  )}
                </>
              )}
            </Tab>
          ))}
        </TabList>

        <TabPanel id="account" className="mt-6">
          <AccountSettingsPanel />
        </TabPanel>
        <TabPanel id="profile" className="mt-6">
          <ProfileSettingsPanel />
        </TabPanel>
        <TabPanel id="storage" className="mt-6">
          <StorageSection />
        </TabPanel>
        <TabPanel id="system" className="mt-6">
          <SystemSection />
        </TabPanel>
        <TabPanel id="appearance" className="mt-6">
          <AppearanceSection />
        </TabPanel>
      </Tabs>
    </div>
  );
}
