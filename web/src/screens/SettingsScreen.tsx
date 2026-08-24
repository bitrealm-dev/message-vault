import { SelectionIndicator, Tab, TabList, TabPanel, Tabs } from "react-aria-components";
import { useSearchParams } from "react-router-dom";
import { parseSelectKey } from "../lib/selectKey";
import { AccountSettingsPanel } from "./settings/AccountSettingsPanel";
import { AppearanceSection } from "./settings/AppearanceSection";
import { ProfileSettingsPanel } from "./settings/ProfileSettingsPanel";
import { StorageSection } from "./settings/StorageSection";
import { SystemSection } from "./settings/SystemSection";

const SETTINGS_TABS = ["account", "profile", "storage", "system", "appearance"] as const;
type SettingsTab = (typeof SETTINGS_TABS)[number];

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "account", label: "Account" },
  { id: "profile", label: "Profile" },
  { id: "storage", label: "Storage" },
  { id: "system", label: "System" },
  { id: "appearance", label: "Appearance" },
];

function tabFromSearchParam(raw: string | null): SettingsTab {
  return parseSelectKey(raw, SETTINGS_TABS) ?? "account";
}

function tabClassName({ isSelected }: { isSelected: boolean }) {
  return `relative -mb-px cursor-pointer border-none bg-transparent px-3 py-2 text-[0.813rem] font-medium outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-accent ${
    isSelected ? "text-text" : "text-muted hover:text-text"
  }`;
}

export default function SettingsScreen() {
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = tabFromSearchParam(searchParams.get("tab"));

  return (
    <div className="max-w-[820px] p-6 text-text">
      <header>
        <h2 className="m-0 text-text">Settings</h2>
        <p className="mt-[0.35rem] text-[0.875rem] text-muted">
          Manage your account, profile, storage, system, and appearance.
        </p>
      </header>

      <Tabs
        selectedKey={tab}
        onSelectionChange={(key) => {
          const next = parseSelectKey(key, SETTINGS_TABS);
          if (!next) return;
          const params = new URLSearchParams(searchParams);
          params.set("tab", next);
          setSearchParams(params, { replace: true });
        }}
      >
        <TabList
          aria-label="Settings sections"
          className="relative mt-5 flex gap-1 border-b border-border"
        >
          {TABS.map((t) => (
            <Tab key={t.id} id={t.id} className={tabClassName}>
              {t.label}
              <SelectionIndicator className="absolute bottom-0 left-2 right-2 h-[2px] rounded-full bg-accent transition-[translate,width] duration-200 motion-reduce:transition-none" />
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
