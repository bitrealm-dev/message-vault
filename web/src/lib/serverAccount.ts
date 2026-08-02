import { redirect } from "next/navigation";

import { requireAccountId } from "./accountContext";
import { runWithAccountAsync } from "./accountScope";
import { accountNeedsOnboarding } from "./onboarding";

export async function withServerAccount<T>(
  fn: (accountId: string) => T | Promise<T>,
  options?: { allowIncomplete?: boolean },
): Promise<T> {
  const accountId = await requireAccountId();
  return runWithAccountAsync(accountId, async () => {
    if (!options?.allowIncomplete && accountNeedsOnboarding(accountId)) {
      redirect("/onboarding");
    }
    return fn(accountId);
  });
}
