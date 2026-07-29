import { AppShell } from "@/components/AppShell";
import { SettingsFrame } from "@/components/SettingsFrame";
import { listLabels } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function SettingsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return withServerAccount(async () => {
    const labels = listLabels();
    return (
      <AppShell active="/settings" labels={labels}>
        <main className="h-full min-h-0 min-w-0 overflow-y-auto bg-bg px-6 py-8 sm:px-8 sm:py-10">
          <SettingsFrame>{children}</SettingsFrame>
        </main>
      </AppShell>
    );
  });
}
