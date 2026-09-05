import { HomePageClient } from "@/components/HomePageClient";
import { homeStats, listLabels } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  return withServerAccount(async () => {
    const [stats, labels] = await Promise.all([homeStats(), listLabels()]);
    return <HomePageClient labels={labels} stats={stats} />;
  });
}
