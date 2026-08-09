"use client";

import { useMessageBadgePrefs } from "./useMessageBadgePrefs";

export function MessageBadgeSettings() {
  const {
    showMessageBadge,
    showGroupMessageBadge,
    showContactInitials,
    showContactDateRange,
    setShowMessageBadge,
    setShowGroupMessageBadge,
    setShowContactInitials,
    setShowContactDateRange,
  } = useMessageBadgePrefs();

  return (
    <>
      <section className="max-w-xl">
        <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
          List badges
        </h2>
        <p className="mt-1 text-[13px] text-muted">
          Choose what appears in the contact list. Changes save automatically.
        </p>

        <div className="mt-4 space-y-3">
          <label className="flex cursor-pointer items-center gap-2.5 text-[14px] text-text">
            <input
              type="checkbox"
              className="checkbox-list"
              checked={showContactDateRange}
              onChange={(e) => setShowContactDateRange(e.target.checked)}
            />
            Show message date range
          </label>
          <label className="flex cursor-pointer items-center gap-2.5 text-[14px] text-text">
            <input
              type="checkbox"
              className="checkbox-list"
              checked={showMessageBadge}
              onChange={(e) => setShowMessageBadge(e.target.checked)}
            />
            Show direct message count
          </label>
          <label className="flex cursor-pointer items-center gap-2.5 text-[14px] text-text">
            <input
              type="checkbox"
              className="checkbox-list"
              checked={showGroupMessageBadge}
              onChange={(e) => setShowGroupMessageBadge(e.target.checked)}
            />
            Show group messages icon
          </label>
        </div>
      </section>

      <section className="mt-10 max-w-xl">
        <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
          Contact initials
        </h2>
        <p className="mt-1 text-[13px] text-muted">
          Show initials beside contacts.
        </p>

        <div className="mt-4 space-y-3">
          <label className="flex cursor-pointer items-center gap-2.5 text-[14px] text-text">
            <input
              type="checkbox"
              className="checkbox-list"
              checked={showContactInitials}
              onChange={(e) => setShowContactInitials(e.target.checked)}
            />
            Show contact initials
          </label>
        </div>
      </section>
    </>
  );
}
