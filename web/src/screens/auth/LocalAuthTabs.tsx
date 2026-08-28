import { SelectionIndicator, Tab, TabList, TabPanel, Tabs } from "react-aria-components";
import CreateAccountForm from "./CreateAccountForm";
import LoginForm from "./LoginForm";

const AUTH_TABS = [
  { id: "login", label: "Login" },
  { id: "create", label: "Create Account" },
] as const;

function tabClassName({ isSelected }: { isSelected: boolean }) {
  return `relative -mb-px flex-1 cursor-pointer border-none bg-transparent px-3 py-2 text-center text-[0.875rem] font-medium outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-accent ${
    isSelected ? "text-text" : "text-muted hover:text-text"
  }`;
}

/**
 * The two ways into a vault in local auth mode. Each panel keeps its own busy
 * and error state, so switching tabs leaves the other form's message behind.
 */
export default function LocalAuthTabs({
  serverUrl,
  disabled = false,
}: {
  serverUrl: string;
  disabled?: boolean;
}) {
  return (
    <Tabs defaultSelectedKey="login" className="flex min-h-0 flex-1 flex-col">
      <TabList
        aria-label="Sign in or create an account"
        className="relative mb-6 flex border-b border-border"
      >
        {AUTH_TABS.map((t) => (
          <Tab key={t.id} id={t.id} className={tabClassName}>
            {t.label}
            <SelectionIndicator className="absolute bottom-0 left-2 right-2 h-[2px] rounded-full bg-accent transition-[translate,width] duration-200 motion-reduce:transition-none" />
          </Tab>
        ))}
      </TabList>

      <TabPanel id="login" className="flex min-h-0 flex-1 flex-col outline-none">
        <LoginForm serverUrl={serverUrl} disabled={disabled} />
      </TabPanel>
      <TabPanel id="create" className="flex min-h-0 flex-1 flex-col outline-none">
        <CreateAccountForm serverUrl={serverUrl} disabled={disabled} />
      </TabPanel>
    </Tabs>
  );
}
