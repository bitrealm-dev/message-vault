"use client";

import {
  resolveSettingsReturnTo,
  SETTINGS_TABS,
  settingsTabActive,
  settingsTabHref,
} from "@/lib/settingsNav";
import { ChevronRightIcon } from "@/components/icons";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { Suspense, type ReactNode } from "react";

function SettingsFrameBody({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const returnTo = resolveSettingsReturnTo(searchParams.get("returnTo"));

  return (
    <div className="mx-auto w-full max-w-2xl">
      <header className="mb-6">
        <Link
          href={returnTo}
          className="mb-4 inline-flex items-center gap-1 rounded-md py-1 pr-2 pl-1 text-[13px] font-medium text-muted transition-colors hover:bg-hover hover:text-text"
        >
          <ChevronRightIcon className="size-3.5 rotate-180 opacity-80" />
          Back
        </Link>
        <h1 className="text-2xl font-semibold tracking-tight text-text">
          Settings
        </h1>
        <p className="mt-1 text-[14px] text-muted">
          Manage your profile, access, storage, and appearance.
        </p>
        <nav
          aria-label="Settings sections"
          className="mt-5 flex gap-1 border-b border-border"
        >
          {SETTINGS_TABS.map((tab) => {
            const active = settingsTabActive(pathname, tab.href);
            const href = settingsTabHref(
              tab.href,
              searchParams.get("returnTo"),
            );
            return (
              <Link
                key={tab.href}
                href={href}
                // Avoid caching a logged-out redirect for these tabs (Next can
                // reuse a pre-login prefetch and bounce back to /login).
                prefetch={false}
                className={`relative -mb-px px-3 py-2 text-[13px] font-medium transition-colors ${
                  active
                    ? "text-text"
                    : "text-muted hover:text-text"
                }`}
              >
                {tab.label}
                {active && (
                  <span
                    aria-hidden
                    className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-accent"
                  />
                )}
              </Link>
            );
          })}
        </nav>
      </header>
      {children}
    </div>
  );
}

/** Shared Settings page chrome: title, tabs, and content. */
export function SettingsFrame({ children }: { children: ReactNode }) {
  return (
    <Suspense
      fallback={
        <div className="mx-auto w-full max-w-2xl">
          <header className="mb-6">
            <h1 className="text-2xl font-semibold tracking-tight text-text">
              Settings
            </h1>
          </header>
          {children}
        </div>
      }
    >
      <SettingsFrameBody>{children}</SettingsFrameBody>
    </Suspense>
  );
}
