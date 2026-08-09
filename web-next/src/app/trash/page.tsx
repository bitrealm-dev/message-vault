import { BrowsePageLayout } from "@/components/BrowsePageLayout";
import { TrashShell } from "@/components/TrashShell";
import {
  listLabels,
  listTrashedContactMessages,
  listTrashedContacts,
  listTrashedGroupYearRows,
  listTrashedHandles,
} from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function TrashPage({
  searchParams,
}: {
  searchParams: Promise<{ h?: string; g?: string; y?: string; tab?: string }>;
}) {
  const sp = await searchParams;
  const initialHandle = sp.h?.trim() || null;
  const rawG = sp.g ? Number(sp.g) : null;
  const initialConversationId = Number.isFinite(rawG) ? rawG : null;
  const rawY = sp.y ? Number(sp.y) : null;
  const initialYear = Number.isFinite(rawY) ? rawY : null;

  return withServerAccount(async () => {
    const handles = listTrashedHandles();
    const groupChats = listTrashedGroupYearRows();
    const trashedContacts = listTrashedContacts();
    const trashedContactMessages = listTrashedContactMessages();
    const labels = listLabels();

    return (
      <BrowsePageLayout active="/trash" labels={labels}>
        <TrashShell
          handles={handles}
          groupChats={groupChats}
          trashedContacts={trashedContacts}
          trashedContactMessages={trashedContactMessages}
          initialHandle={initialHandle}
          initialConversationId={initialConversationId}
          initialYear={initialYear}
        />
      </BrowsePageLayout>
    );
  });
}
