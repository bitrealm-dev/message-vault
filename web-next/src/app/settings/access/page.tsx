import { SettingsAccessForm } from "@/components/SettingsAccessForm";
import { getAuthMode, getHankoApiUrl } from "@/lib/authMode";

export default function SettingsAccessPage() {
  return (
    <SettingsAccessForm
      authMode={getAuthMode()}
      hankoApiUrl={getHankoApiUrl()}
    />
  );
}
