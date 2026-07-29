import { BrowsePageLayout } from "@/components/BrowsePageLayout";
import { GroupMessagesShell } from "@/components/GroupMessagesShell";
import { listGroupYearRows, listLabels } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function GroupMessagesPage({
  searchParams,
}: {
  searchParams: Promise<{ g?: string; y?: string }>;
}) {
  const sp = await searchParams;
  const rawG = sp.g ? Number(sp.g) : null;
  const conversationId = Number.isFinite(rawG) ? rawG : null;
  const rawY = sp.y ? Number(sp.y) : null;
  const year = Number.isFinite(rawY) ? rawY : null;

  return withServerAccount(async () => {
    const groupChats = listGroupYearRows();
    const labels = listLabels();

    return (
      <BrowsePageLayout active="/group-messages" labels={labels}>
        <GroupMessagesShell
          groupChats={groupChats}
          initialConversationId={conversationId}
          initialYear={year}
        />
      </BrowsePageLayout>
    );
  });
}
