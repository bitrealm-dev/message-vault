"use client";

import { isSettingsPath } from "@/lib/settingsNav";
import { useEffect } from "react";
import {
  AddressBookIcon,
  GroupMessagesOutlineIcon,
  HomeIcon,
  PanelCollapseIcon,
  PanelExpandIcon,
  PersonDetailIcon,
  ProhibitedIcon,
  SettingsIcon,
  TrashIcon,
} from "./icons";
import { IconHoverTarget } from "./IconHoverLabel";
import { LabelsNav, SidebarNavLink } from "./LabelsNav";
import { useVaultTitleActions } from "./useVaultTitleActions";
import { VaultTitleMenu } from "./VaultTitleMenu";

const navHeadingClass = "px-3 pb-1";

export function AppSidebar({
  active,
  labels = [],
  collapsed,
  animating = false,
  onHideNav,
  onShowNav,
  onExpandLabels,
  focusLabelsToken = 0,
}: {
  active: string;
  labels?: string[];
  collapsed: boolean;
  /** True while the drawer width is animating. */
  animating?: boolean;
  onHideNav?: () => void;
  onShowNav?: () => void;
  onExpandLabels?: () => void;
  /** Increment to scroll the Labels section into view after expand. */
  focusLabelsToken?: number;
}) {
  const {
    demoResetAvailable,
    resettingDemo,
    resetError,
    logout,
    resetDemo,
    confirmDialog,
  } = useVaultTitleActions();

  useEffect(() => {
    if (collapsed || focusLabelsToken === 0) return;
    const el = document.getElementById("nav-labels");
    if (!el) return;
    requestAnimationFrame(() => {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    });
  }, [collapsed, focusLabelsToken]);

  const groupMessagesIcon = (
    <GroupMessagesOutlineIcon className="size-5 shrink-0 opacity-80" />
  );
  const settingsActive = isSettingsPath(active);

  return (
    <aside className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-sidebar">
      <div className="flex h-[45px] shrink-0 items-center gap-1 border-b border-border px-3">
        {collapsed ? (
          <IconHoverTarget label="Show navigation" placement="right">
            <button
              type="button"
              aria-label="Show navigation"
              onClick={onShowNav}
              className="shrink-0 rounded-md p-1.5 text-muted transition-colors hover:bg-hover hover:text-text"
            >
              <PanelExpandIcon className="size-5" />
            </button>
          </IconHoverTarget>
        ) : (
          <>
            {onHideNav && (
              <IconHoverTarget label="Hide navigation" placement="bottom">
                <button
                  type="button"
                  aria-label="Hide navigation"
                  onClick={onHideNav}
                  className="shrink-0 rounded-md p-1.5 text-muted transition-colors hover:bg-hover hover:text-text"
                >
                  <PanelCollapseIcon className="size-5" />
                </button>
              </IconHoverTarget>
            )}
            <VaultTitleMenu
              demoResetAvailable={demoResetAvailable}
              resettingDemo={resettingDemo}
              onResetDemo={() => void resetDemo()}
              onLogout={() => void logout()}
            />
          </>
        )}
      </div>

      {/* Aligns nav with contact-list body (search + toolbar are both 45px). */}
      <div className="h-[45px] shrink-0 border-b border-border" aria-hidden />

      {!collapsed && resetError && (
        <p className="shrink-0 px-3 py-1.5 text-[11px] text-danger" role="alert">
          {resetError}
        </p>
      )}

      <nav className="flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto pb-2">
        <SidebarNavLink
          href="/"
          label="Home"
          active={active === "/"}
          compact={collapsed}
          icon={<HomeIcon className="size-5 shrink-0 opacity-80" />}
        />

        {!collapsed && (
          <div className={`mt-3 flex h-8 items-center ${navHeadingClass}`}>
            <span className="text-[12px] font-semibold tracking-wider text-muted uppercase">
              Messages
            </span>
          </div>
        )}
        {collapsed && <div className="mt-3" aria-hidden />}
        <SidebarNavLink
          href="/all"
          label="All"
          active={active === "/all"}
          compact={collapsed}
          icon={<PersonDetailIcon className="size-5 shrink-0 opacity-80" />}
        />
        <SidebarNavLink
          href="/contacts"
          label="Active"
          active={active === "/contacts"}
          compact={collapsed}
          icon={<AddressBookIcon className="size-5 shrink-0 opacity-80" />}
        />
        <SidebarNavLink
          href="/excluded"
          label="Inactive"
          active={active === "/excluded"}
          compact={collapsed}
          icon={<ProhibitedIcon className="size-5 shrink-0 opacity-80" />}
        />
        <SidebarNavLink
          href="/group-messages"
          label="Group Messages"
          active={active === "/group-messages"}
          compact={collapsed}
          icon={groupMessagesIcon}
        />

        <div className="mt-3" aria-hidden />

        <LabelsNav
          labels={labels}
          compact={collapsed}
          hideItems={collapsed || animating}
          onExpandLabels={onExpandLabels}
        />

        <div className="mt-3" aria-hidden />

        <SidebarNavLink
          href="/trash"
          label="Trash"
          active={active === "/trash"}
          compact={collapsed}
          icon={<TrashIcon className="size-5 shrink-0 opacity-80" />}
        />
        <SidebarNavLink
          href="/settings/account"
          label="Settings"
          active={settingsActive}
          compact={collapsed}
          icon={<SettingsIcon className="size-5 shrink-0 opacity-80" />}
        />
      </nav>
      {confirmDialog}
    </aside>
  );
}
