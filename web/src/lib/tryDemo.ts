import { apiClient, setBaseUrl } from "./api";
import type { SessionResponse } from "./authGuards";

/**
 * Open the shared sample account on `serverUrl`.
 *
 * No sign-in screen offers this any more — the "Try it" buttons were taken out
 * of the login cards — but the server route and this client call are kept so
 * the sample vault can be put back in front of users without rebuilding it.
 */
export function signInToDemoAccount(serverUrl: string): Promise<SessionResponse> {
  const url = serverUrl.trim();
  setBaseUrl(url);
  return apiClient.post<SessionResponse>("/v1/auth/try-demo", {});
}
