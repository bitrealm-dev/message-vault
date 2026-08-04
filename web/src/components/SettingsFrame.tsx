"use client";

import { SETTINGS_TABS, settingsTabActive } from "@/lib/settingsNav";
import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";

/** Shared Settings page chrome: title, tabs, and content. */
export function SettingsFrame({ children }: { children: ReactNode }) {
  const pathname = usePathname();

  return (
    <div className="mx-auto w-full max-w-2xl">
      <header className="mb-6">
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
            return (
              <Link
                key={tab.href}
                href={tab.href}
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
