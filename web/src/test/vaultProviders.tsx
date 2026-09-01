/**
 * Render a component that fetches vault data.
 *
 * Anything using `useVaultQuery` needs two things from the tree: a query client
 * to cache into, and a signed-in account to name the cache entry after. This
 * supplies the first. The second comes from `useAuth`, which tests fake in the
 * usual way — see `mockedAuth` below for the shape.
 *
 * The client is built fresh per call, so no test can read a cache another test
 * filled, and retries are off so a rejected request fails the test at once
 * rather than after a delay.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type RenderOptions, type RenderResult, render } from "@testing-library/react";
import type { ReactElement, ReactNode } from "react";

/** A query client with retries and background refetching off. */
export function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  });
}

/** Wrap children in a fresh query client. */
export function VaultProviders({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={testQueryClient()}>{children}</QueryClientProvider>;
}

/** `render`, with the query client a vault query needs. */
export function renderWithVault(ui: ReactElement, options?: RenderOptions): RenderResult {
  return render(ui, { wrapper: VaultProviders, ...options });
}

/**
 * What `vi.mock("<path>/auth")` should return so a vault query has an account
 * to name its cache entry after.
 *
 * Any id works; what matters is that it is stable within a test, since a
 * changing account id is a different cache entry by design.
 */
export const mockedAuth = {
  accountId: "test-account",
  token: "test-token",
  isAuthenticated: true,
};
