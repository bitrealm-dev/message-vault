import { SelectionIndicator, Tab, TabList, TabPanel, Tabs } from "react-aria-components";
import { useSearchParams } from "react-router-dom";
import { canUseConvert } from "../lib/desktopFeatures";
import { parseSelectKey } from "../lib/selectKey";
import { isTauri } from "../lib/tauri-check";
import { useAccountProfile } from "../lib/useAccountProfile";
import { AccountSettingsPanel } from "./settings/AccountSettingsPanel";
import { AdminUsersPanel } from "./settings/AdminUsersPanel";
import { AppearanceSection } from "./settings/AppearanceSection";
import { ConvertSection } from "./settings/ConvertSection";
import { ProfileSettingsPanel } from "./settings/ProfileSettingsPanel";
import { StorageSection } from "./settings/StorageSection";
import { SystemSection } from "./settings/SystemSection";

const ALL_TABS = [
  "account",
  "profile",
  "users",
  "storage",
  "system",
  "convert",
  "appearance",
] as const;
type SettingsTab = (typeof ALL_TABS)[number];

const TAB_LABELS: Record<SettingsTab, string> = {
  account: "Account",
  profile: "Profile",
  users: "Users",
  storage: "Storage",
  system: "System",
  convert: "Convert",
  appearance: "Appearance",
};

/**
 * Tabs this person can open, in display order. Users exists for
 * administrators only, so `?tab=users` can never land a non-admin on a panel
 * that will 403. Convert is a desktop-only tool: it runs `message-reexport`
 * in the desktop process, so a browser visiting the website never sees it.
 */
function visibleTabs(isAdmin: boolean, isDesktop: boolean): SettingsTab[] {
  return ALL_TABS.filter((id) => {
    if (id === "users") return isAdmin;
    if (id === "convert") return canUseConvert(isDesktop);
    return true;
  });
}

/** "account, profile, and appearance" — the header sentence built from the visible tabs. */
function tabSummary(tabs: SettingsTab[]): string {
  const names = tabs.map((id) => TAB_LABELS[id].toLowerCase());
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")}, and ${names[names.length - 1]}`;
}

function tabFromSearchParam(raw: string | null, allowed: readonly SettingsTab[]): SettingsTab {
  return parseSelectKey(raw, allowed) ?? "account";
}

function tabClassName({ isSelected }: { isSelected: boolean }) {
  return `relative -mb-px cursor-pointer border-none bg-transparent px-3 py-2 text-[0.813rem] font-medium outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-accent ${
    isSelected ? "text-text" : "text-muted hover:text-text"
  }`;
}

export default function SettingsScreen() {
  const [searchParams, setSearchParams] = useSearchParams();
  const { profile } = useAccountProfile();
  const isAdmin = profile?.is_admin === true;
  const tabs = visibleTabs(isAdmin, isTauri());
  const tab = tabFromSearchParam(searchParams.get("tab"), tabs);

  return (
    <div className="max-w-[820px] p-6 text-text">
      <header>
        <h2 className="m-0 text-text">Settings</h2>
        <p className="mt-[0.35rem] text-[0.875rem] text-muted">Manage your {tabSummary(tabs)}.</p>
      </header>

      <Tabs
        selectedKey={tab}
        onSelectionChange={(key) => {
          const next = parseSelectKey(key, tabs);
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
          {tabs.map((id) => (
            <Tab key={id} id={id} className={tabClassName}>
              {TAB_LABELS[id]}
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
        {isAdmin ? (
          <TabPanel id="users" className="mt-6">
            <AdminUsersPanel />
          </TabPanel>
        ) : null}
        <TabPanel id="storage" className="mt-6">
          <StorageSection />
        </TabPanel>
        <TabPanel id="system" className="mt-6">
          <SystemSection />
        </TabPanel>
        {tabs.includes("convert") ? (
          <TabPanel id="convert" className="mt-6">
            <ConvertSection />
          </TabPanel>
        ) : null}
        <TabPanel id="appearance" className="mt-6">
          <AppearanceSection />
        </TabPanel>
      </Tabs>
    </div>
  );
}
