/**
 * Every vault route the web app calls, one named function each.
 *
 * This is the only module that knows a vault URL. Screens call
 * `listConversations` rather than writing `/v1/conversations?…`, so renaming a
 * route is a change here and nowhere else, and no test has to match on a path.
 *
 * Request and response types come from `vaultApi.types.ts`, which is generated
 * from `docs/src/assets/openapi.json` — the document a vault-side test pins to
 * the running server. Regenerate with `npm run gen:api`; `scripts/check-pr.sh`
 * fails when the checked-in file is out of date.
 *
 * These functions only talk to the vault. Caching, request deduplication, and
 * telling the rest of the app that something changed all belong to TanStack
 * Query above this layer. See
 * `docs/adr/0002-one-way-to-fetch-data-in-the-web-app.md`.
 *
 * Routes reachable only from the desktop app's Rust side — asset upload and
 * `POST /v1/import` — have no function here, because nothing in the browser
 * calls them.
 */

import { type ApiRequestOptions, apiClient } from "./api";
import type { components } from "./vaultApi.types";

type Schema = components["schemas"];

/** Options every read accepts, so a caller can cancel an in-flight request. */
export type VaultRequestOptions = ApiRequestOptions;

/**
 * Build a query string from values that may be absent.
 *
 * Keys whose value is `undefined`, `null`, or an empty string are dropped, so
 * a caller can pass its whole filter object without pruning it first. The
 * result has no leading `?`; callers that need one add it.
 */
function query(params: Record<string, string | number | boolean | undefined | null>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") continue;
    search.set(key, String(value));
  }
  return search.toString();
}

/** Append a query string only when it has something in it. */
function withQuery(path: string, qs: string): string {
  return qs ? `${path}?${qs}` : path;
}

// ── Auth ────────────────────────────────────────────────────────────────────

export function login(body: Schema["LoginRequest"]): Promise<Schema["AuthTokenResponse"]> {
  return apiClient.post<Schema["AuthTokenResponse"]>("/v1/auth/login", body);
}

export function register(body: Schema["RegisterRequest"]): Promise<Schema["AuthTokenResponse"]> {
  return apiClient.post<Schema["AuthTokenResponse"]>("/v1/auth/register", body);
}

export function checkAuth(opts?: VaultRequestOptions): Promise<Schema["AuthCheckResponse"]> {
  return apiClient.get<Schema["AuthCheckResponse"]>("/v1/auth/check", opts);
}

export function logout(opts?: VaultRequestOptions): Promise<Schema["LogoutResponse"]> {
  return apiClient.post<Schema["LogoutResponse"]>("/v1/auth/logout", {}, opts);
}

export function changePassword(
  body: Schema["ChangePasswordRequest"],
): Promise<Schema["ChangePasswordResponse"]> {
  return apiClient.post<Schema["ChangePasswordResponse"]>("/v1/auth/change-password", body);
}

export function deleteAccount(
  body: Schema["DeleteAccountRequest"],
): Promise<Schema["DeleteAccountResponse"]> {
  return apiClient.post<Schema["DeleteAccountResponse"]>("/v1/auth/delete-account", body);
}

// ── Account ─────────────────────────────────────────────────────────────────

export function getAccountProfile(
  opts?: VaultRequestOptions,
): Promise<Schema["AccountProfileResponse"]> {
  return apiClient.get<Schema["AccountProfileResponse"]>("/v1/account/profile", opts);
}

export function updateAccountProfile(
  body: Schema["AccountProfileUpdateRequest"],
): Promise<Schema["AccountProfileResponse"]> {
  return apiClient.post<Schema["AccountProfileResponse"]>("/v1/account/profile", body);
}

export function getAccountStorage(
  opts?: VaultRequestOptions,
): Promise<Schema["AccountStorageResponse"]> {
  return apiClient.get<Schema["AccountStorageResponse"]>("/v1/account/storage", opts);
}

export function deleteAllMessages(
  body: Schema["DeleteMessagesRequest"],
): Promise<Schema["DeleteMessagesResponse"]> {
  return apiClient.post<Schema["DeleteMessagesResponse"]>("/v1/account/delete-messages", body);
}

// ── API tokens ──────────────────────────────────────────────────────────────

export function listApiTokens(
  opts?: VaultRequestOptions,
): Promise<Schema["ListApiTokensResponse"]> {
  return apiClient.get<Schema["ListApiTokensResponse"]>("/v1/account/api-tokens", opts);
}

export function createApiToken(
  body: Schema["CreateApiTokenRequest"],
): Promise<Schema["CreateApiTokenResponse"]> {
  return apiClient.post<Schema["CreateApiTokenResponse"]>("/v1/account/api-tokens", body);
}

export function renameApiToken(
  id: string,
  body: Schema["RenameApiTokenRequest"],
): Promise<Schema["RenameApiTokenResponse"]> {
  return apiClient.patch<Schema["RenameApiTokenResponse"]>(
    `/v1/account/api-tokens/${encodeURIComponent(id)}`,
    body,
  );
}

export function deleteApiToken(id: string): Promise<Schema["DeleteApiTokenResponse"]> {
  return apiClient.delete<Schema["DeleteApiTokenResponse"]>(
    `/v1/account/api-tokens/${encodeURIComponent(id)}`,
  );
}

// ── Administration ──────────────────────────────────────────────────────────

export function listUsers(opts?: VaultRequestOptions): Promise<Schema["ListUsersResponse"]> {
  return apiClient.get<Schema["ListUsersResponse"]>("/v1/admin/users", opts);
}

export function createUser(body: Schema["CreateUserRequest"]): Promise<unknown> {
  return apiClient.post<unknown>("/v1/admin/users", body);
}

export function updateUser(accountId: string, body: Schema["PatchUserRequest"]): Promise<unknown> {
  return apiClient.patch<unknown>(`/v1/admin/users/${encodeURIComponent(accountId)}`, body);
}

export function setUserPassword(
  accountId: string,
  body: Schema["SetPasswordRequest"],
): Promise<unknown> {
  return apiClient.put<unknown>(`/v1/admin/users/${encodeURIComponent(accountId)}/password`, body);
}

export function deleteUser(accountId: string): Promise<unknown> {
  return apiClient.delete<unknown>(`/v1/admin/users/${encodeURIComponent(accountId)}`);
}

export function deleteUserMessages(accountId: string): Promise<unknown> {
  return apiClient.delete<unknown>(`/v1/admin/users/${encodeURIComponent(accountId)}/messages`);
}

// ── Conversations ───────────────────────────────────────────────────────────

/** Filters the conversation list accepts. Absent values are left off the URL. */
export type ConversationListParams = {
  q?: string;
  limit?: number;
  offset?: number;
  sort?: string;
  order?: string;
  count_only?: boolean;
};

export function listConversations(
  params: ConversationListParams,
  opts?: VaultRequestOptions,
): Promise<Schema["ConversationListPage"]> {
  return apiClient.get<Schema["ConversationListPage"]>(
    withQuery("/v1/conversations", query(params)),
    opts,
  );
}

export function getConversationSources(
  conversationId: string,
  opts?: VaultRequestOptions,
): Promise<Schema["ConversationSourcesPage"]> {
  return apiClient.get<Schema["ConversationSourcesPage"]>(
    `/v1/conversations/${encodeURIComponent(conversationId)}/sources`,
    opts,
  );
}

// ── Messages (Export) ───────────────────────────────────────────────────────

export function exportMessages(
  params: { q: string; offset?: number; limit?: number },
  opts?: VaultRequestOptions,
): Promise<Schema["ExportMessagesResponse"]> {
  return apiClient.get<Schema["ExportMessagesResponse"]>(
    withQuery("/v1/export/messages", query(params)),
    opts,
  );
}

export function countExportMessages(
  params: { q: string; source?: string },
  opts?: VaultRequestOptions,
): Promise<Schema["ExportCountResponse"]> {
  return apiClient.get<Schema["ExportCountResponse"]>(
    withQuery("/v1/export/messages/count", query(params)),
    opts,
  );
}

// ── Contacts ────────────────────────────────────────────────────────────────

export type ContactListParams = { q?: string; limit?: number; offset?: number };

export function listContacts(
  params: ContactListParams,
  opts?: VaultRequestOptions,
): Promise<Schema["ContactListPage"]> {
  return apiClient.get<Schema["ContactListPage"]>(withQuery("/v1/contacts", query(params)), opts);
}

export function getContact(
  contactId: string | number,
  opts?: VaultRequestOptions,
): Promise<Schema["ContactDetail"]> {
  return apiClient.get<Schema["ContactDetail"]>(
    `/v1/contacts/${encodeURIComponent(String(contactId))}`,
    opts,
  );
}

/**
 * Change one thing about a contact: its preferred name, or one handle added,
 * updated, or removed. The vault answers with the contact as it now stands.
 */
export function updateContact(
  contactId: string | number,
  body: Schema["ContactMutationBody"],
): Promise<Schema["ContactDetail"]> {
  return apiClient.patch<Schema["ContactDetail"]>(
    `/v1/contacts/${encodeURIComponent(String(contactId))}`,
    body,
  );
}

export function getContactSummaries(
  body: Schema["ContactSummariesBody"],
  opts?: VaultRequestOptions,
): Promise<Schema["ContactSummariesPage"]> {
  return apiClient.post<Schema["ContactSummariesPage"]>("/v1/contacts/summaries", body, opts);
}

export function matchContacts(
  body: Schema["ContactMatchBody"],
): Promise<Schema["ContactMatchResponse"]> {
  return apiClient.post<Schema["ContactMatchResponse"]>("/v1/contacts/match", body);
}

export function loadAddressBook(
  body: Schema["AddressBookBody"],
): Promise<Schema["AddressBookLoadResponse"]> {
  return apiClient.post<Schema["AddressBookLoadResponse"]>("/v1/contacts/address-book", body);
}

// ── Contact Groups ──────────────────────────────────────────────────────────

export function listContactGroups(
  opts?: VaultRequestOptions,
): Promise<Schema["ContactGroupsListResponse"]> {
  return apiClient.get<Schema["ContactGroupsListResponse"]>("/v1/contact-groups", opts);
}

export function createContactGroup(
  body: Schema["ContactGroupNameBody"],
): Promise<Schema["ContactGroupNamedListResponse"]> {
  return apiClient.post<Schema["ContactGroupNamedListResponse"]>("/v1/contact-groups", body);
}

export function renameContactGroup(
  body: Schema["ContactGroupRenameBody"],
): Promise<Schema["ContactGroupNamedListResponse"]> {
  return apiClient.patch<Schema["ContactGroupNamedListResponse"]>("/v1/contact-groups", body);
}

export function deleteContactGroup(
  body: Schema["ContactGroupNameBody"],
): Promise<Schema["ContactGroupDeleteResponse"]> {
  return apiClient.delete<Schema["ContactGroupDeleteResponse"]>("/v1/contact-groups", body);
}

export function setContactGroupMembership(
  body: Schema["ContactGroupMembershipBody"],
): Promise<Schema["MembershipChangedResponse"]> {
  return apiClient.post<Schema["MembershipChangedResponse"]>("/v1/contacts/groups", body);
}

// ── Message Tags ────────────────────────────────────────────────────────────

export function listMessageTags(
  opts?: VaultRequestOptions,
): Promise<Schema["MessageTagsListResponse"]> {
  return apiClient.get<Schema["MessageTagsListResponse"]>("/v1/message-tags", opts);
}

export function createMessageTag(
  body: Schema["MessageTagNameBody"],
): Promise<Schema["MessageTagNamedListResponse"]> {
  return apiClient.post<Schema["MessageTagNamedListResponse"]>("/v1/message-tags", body);
}

export function renameMessageTag(
  body: Schema["MessageTagRenameBody"],
): Promise<Schema["MessageTagNamedListResponse"]> {
  return apiClient.patch<Schema["MessageTagNamedListResponse"]>("/v1/message-tags", body);
}

export function deleteMessageTag(
  body: Schema["MessageTagNameBody"],
): Promise<Schema["MessageTagDeleteResponse"]> {
  return apiClient.delete<Schema["MessageTagDeleteResponse"]>("/v1/message-tags", body);
}

export function setMessageTagMembership(
  body: Schema["MessageTagMembershipBody"],
): Promise<Schema["MembershipChangedResponse"]> {
  return apiClient.post<Schema["MembershipChangedResponse"]>("/v1/conversations/tags", body);
}

// ── Saved Searches ──────────────────────────────────────────────────────────

export function listSavedSearches(
  opts?: VaultRequestOptions,
): Promise<Schema["SavedSearchesListResponse"]> {
  return apiClient.get<Schema["SavedSearchesListResponse"]>("/v1/saved-searches", opts);
}

export function createSavedSearch(
  body: Schema["SavedSearchBody"],
): Promise<Schema["SavedSearchResponse"]> {
  return apiClient.post<Schema["SavedSearchResponse"]>("/v1/saved-searches", body);
}

export function updateSavedSearch(
  id: number,
  body: Schema["SavedSearchBody"],
): Promise<Schema["SavedSearchResponse"]> {
  return apiClient.patch<Schema["SavedSearchResponse"]>(`/v1/saved-searches/${id}`, body);
}

export function deleteSavedSearch(id: number): Promise<Schema["SavedSearchDeleteResponse"]> {
  return apiClient.delete<Schema["SavedSearchDeleteResponse"]>(`/v1/saved-searches/${id}`);
}

// ── Import Runs ─────────────────────────────────────────────────────────────

export function listImports(opts?: VaultRequestOptions): Promise<Schema["ImportsListResponse"]> {
  return apiClient.get<Schema["ImportsListResponse"]>("/v1/imports", opts);
}

export function getImport(
  id: number,
  opts?: VaultRequestOptions,
): Promise<Schema["ImportDetailResponse"]> {
  return apiClient.get<Schema["ImportDetailResponse"]>(`/v1/imports/${id}`, opts);
}

export function getActiveImport(
  opts?: VaultRequestOptions,
): Promise<Schema["ActiveImportResponse"]> {
  return apiClient.get<Schema["ActiveImportResponse"]>("/v1/imports/active", opts);
}

export function createImport(
  body: Schema["CreateImportBody"],
): Promise<Schema["CreateImportResponse"]> {
  return apiClient.post<Schema["CreateImportResponse"]>("/v1/imports", body);
}

export function setImportStage(
  id: number,
  body: Schema["SetImportStageBody"],
): Promise<Schema["SetImportStageResponse"]> {
  return apiClient.post<Schema["SetImportStageResponse"]>(`/v1/imports/${id}/stage`, body);
}

export function completeImport(
  id: number,
  body: Schema["CompleteImportBody"],
): Promise<Schema["CompleteImportResponse"]> {
  return apiClient.post<Schema["CompleteImportResponse"]>(`/v1/imports/${id}/complete`, body);
}

export function discardImport(id: number): Promise<Schema["DiscardImportResponse"]> {
  return apiClient.post<Schema["DiscardImportResponse"]>(`/v1/imports/${id}/discard`, {});
}

export function getImportContacts(
  id: number,
  opts?: VaultRequestOptions,
): Promise<Schema["ImportContactsResponse"]> {
  return apiClient.get<Schema["ImportContactsResponse"]>(`/v1/imports/${id}/contacts`, opts);
}
