import type { components } from "./vaultApi.types";

/**
 * Account profile as the vault returns it from `GET /v1/account/profile`.
 *
 * Generated from the vault's own OpenAPI document rather than written here, so
 * a field renamed on the server becomes a build error instead of a screen that
 * silently shows nothing.
 */
export type AccountProfile = components["schemas"]["AccountProfileResponse"];
