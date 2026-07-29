import { DateTimeSettings } from "@/components/DateTimeSettings";
import { MessageBadgeSettings } from "@/components/MessageBadgeSettings";
import { ThemeSettings } from "@/components/ThemeSettings";

export default function SettingsDisplayPage() {
  return (
    <div className="space-y-10">
      <MessageBadgeSettings />
      <ThemeSettings />
      <DateTimeSettings />
    </div>
  );
}
