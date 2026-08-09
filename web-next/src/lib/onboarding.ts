import { loadAccountProfile } from "@/lib/accountProfile";

/** True when preferred name or at least one phone is still missing. */
export function accountNeedsOnboarding(accountId: string): boolean {
  const profile = loadAccountProfile(accountId);
  const name = profile.preferred_name?.trim() ?? "";
  return !name || profile.phones.length === 0;
}
