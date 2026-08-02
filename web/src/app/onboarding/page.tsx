import { redirect } from "next/navigation";

import { OnboardingForm } from "@/components/OnboardingForm";
import { accountNeedsOnboarding } from "@/lib/onboarding";
import { withServerAccount } from "@/lib/serverAccount";

export const dynamic = "force-dynamic";

export default async function OnboardingPage() {
  return withServerAccount(
    async (accountId) => {
      if (!accountNeedsOnboarding(accountId)) {
        redirect("/");
      }
      return <OnboardingForm />;
    },
    { allowIncomplete: true },
  );
}
