import { AsyncLocalStorage } from "node:async_hooks";

const storage = new AsyncLocalStorage<string>();

export function runWithAccount<T>(accountId: string, fn: () => T): T {
  return storage.run(accountId, fn);
}

export async function runWithAccountAsync<T>(
  accountId: string,
  fn: () => T | Promise<T>,
): Promise<T> {
  return storage.run(accountId, fn);
}

export function currentAccountId(): string {
  const id = storage.getStore();
  if (!id) {
    throw new Error("No account in scope");
  }
  return id;
}
