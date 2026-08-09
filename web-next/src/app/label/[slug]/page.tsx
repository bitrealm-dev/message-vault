import { ContactBrowsePage, parseContactId } from "@/components/ContactBrowsePage";
import { labelFromSlug } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";
import { notFound } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LabelPage({
  params,
  searchParams,
}: {
  params: Promise<{ slug: string }>;
  searchParams: Promise<{ c?: string }>;
}) {
  const { slug } = await params;
  const sp = await searchParams;
  const label = await withServerAccount(() => labelFromSlug(slug));
  if (!label) notFound();

  return (
    <ContactBrowsePage
      section={{ label }}
      label={label}
      nav={`/label/${slug}`}
      contactId={parseContactId(sp.c)}
    />
  );
}
