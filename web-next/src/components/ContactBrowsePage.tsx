import { BrowsePageLayout } from "@/components/BrowsePageLayout";
import { BrowseShell } from "@/components/BrowseShell";
import { listContacts, listLabels, labelSlug } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";
import type { ContactSection } from "@/lib/types";

export async function ContactBrowsePage({
  section,
  label,
  nav,
  contactId,
}: {
  section: ContactSection;
  label: string;
  nav: string;
  contactId: number | null;
}) {
  return withServerAccount(async () => {
    // The vault gives every imported handle a contact at import time, so
    // there is no "ensure Unknown contacts" step before listing.
    const [contacts, labels] = await Promise.all([
      listContacts(section),
      listLabels(),
    ]);
    const paneKey =
      typeof section === "object" ? `label-${labelSlug(section.label)}` : section;

    return (
      <BrowsePageLayout active={nav} labels={labels}>
        <BrowseShell
          paneStorageKey={paneKey}
          sectionLabel={label}
          contactSection={section}
          contacts={contacts}
          allLabels={labels}
          initialContactId={contactId}
        />
      </BrowsePageLayout>
    );
  });
}

export function parseContactId(raw: string | undefined): number | null {
  if (!raw) return null;
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}
