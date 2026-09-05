import { DateTimeSettings } from "@/components/DateTimeSettings";
import { MessageBadgeSettings } from "@/components/MessageBadgeSettings";
import { ThemeSettings } from "@/components/ThemeSettings";

export default function SettingsDisplayPage() {
  return (
    <div className="space-y-10">
      <p
        role="note"
        className="rounded-lg border border-dashed border-border bg-hover px-3 py-2 text-[12px] opacity-80"
      >
        No /v1 route for display preferences. On this build they are kept in a
        browser cookie and are not shared with the desktop app.
      </p>
      <MessageBadgeSettings />
      <ThemeSettings />
      <DateTimeSettings />
    </div>
  );
}
