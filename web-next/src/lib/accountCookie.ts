/** Session cookie names — kept free of `next/headers` for proxy/edge use. */

/** Holds the account id the vault session belongs to. */
export const ACCOUNT_COOKIE = "mv_account_id";

/** Holds the vault session token from `POST /v1/auth/login`. */
export const SESSION_COOKIE = "mv_session";
