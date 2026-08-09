import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function ExcludedPage({
  searchParams,
}: {
  searchParams: Promise<{ c?: string }>;
}) {
  const sp = await searchParams;
  const query = sp.c ? `?c=${encodeURIComponent(sp.c)}` : "";
  redirect(`/all${query}`);
}
