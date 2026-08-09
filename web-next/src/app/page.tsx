import { HomePageClient } from "@/components/HomePageClient";
import { homeStats, listLabels } from "@/lib/db";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  return withServerAccount(async () => {
    const stats = homeStats();
    const labels = listLabels();
    return <HomePageClient labels={labels} stats={stats} />;
  });
}
