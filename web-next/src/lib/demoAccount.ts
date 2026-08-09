/** Stable id for the seeded demo account (`reset-demo`). */
export const DEMO_ACCOUNT_ID = "00000000-0000-0000-0000-00000000d001";

export function isDemoAccount(accountId: string): boolean {
  return accountId === DEMO_ACCOUNT_ID;
}
