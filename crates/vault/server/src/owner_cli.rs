//! The two shell commands that reach the vault owner's credentials.
//!
//! Claiming a vault and getting back into one are different jobs, so they are
//! different commands: `create-owner` refuses a vault that already has an
//! owner, `reset-owner-password` refuses one that does not. Neither can be
//! mistaken for the other, so setting up a vault cannot silently overwrite a
//! live owner's password.
//!
//! A shell on the server is the right credential for both. Nothing inside the
//! product can reset the owner's password, because no account stands above
//! the owner. See `docs/adr/0008-the-vault-owner-holds-no-messages.md`.

use anyhow::{Result, bail};

use crate::config::Config;
use crate::db::account_profile;
use crate::db::engine::DbTarget;

/// Create the vault owner, claiming an unclaimed vault.
///
/// # Errors
///
/// Fails when the vault already has an owner, when the username is malformed
/// or taken, when the password is shorter than the vault's policy allows, or
/// when the database cannot be opened.
pub async fn create_owner(
    config: &std::path::Path,
    db_url: Option<&str>,
    username: &str,
    password: &str,
) -> Result<String> {
    let cfg = Config::load(config)?;
    let url = db_url.or(cfg.database.url.as_deref());
    let pool = DbTarget::new(url, &cfg.paths.db).open().await?;
    let mut conn = pool.acquire().await?;

    let username = crate::auth::normalize_username(username);
    if !crate::auth::is_valid_username(&username) {
        pool.close().await;
        bail!("username must be 1–128 chars (alphanumeric, _, -, .)");
    }
    if let Err(e) = crate::auth::validate_password_policy(password) {
        pool.close().await;
        bail!("{e}");
    }

    if account_profile::vault_is_claimed(&mut conn).await? {
        pool.close().await;
        bail!(
            "this vault already has an owner; use `reset-owner-password` to set a new password for it"
        );
    }
    if let Err(e) = crate::auth::require_username_free(&mut conn, &username).await {
        pool.close().await;
        bail!("{e}");
    }

    let hash = crate::auth::hash_password(password)?;
    account_profile::insert_account(
        &mut conn,
        account_profile::OWNER_ACCOUNT_ID,
        &username,
        Some(&hash),
        None,
    )
    .await?;

    drop(conn);
    pool.close().await;
    Ok(username)
}

/// Set a new password for an existing vault owner and end their sessions.
///
/// Returns the owner's username, which is as easy to forget as the password
/// and just as unreachable from inside the product.
///
/// # Errors
///
/// Fails when the vault has no owner, when the password is shorter than the
/// vault's policy allows, or when the database cannot be opened.
pub async fn reset_owner_password(
    config: &std::path::Path,
    db_url: Option<&str>,
    password: &str,
) -> Result<String> {
    let cfg = Config::load(config)?;
    let url = db_url.or(cfg.database.url.as_deref());
    let pool = DbTarget::new(url, &cfg.paths.db).open().await?;
    let mut conn = pool.acquire().await?;

    if let Err(e) = crate::auth::validate_password_policy(password) {
        pool.close().await;
        bail!("{e}");
    }
    if !account_profile::vault_is_claimed(&mut conn).await? {
        pool.close().await;
        bail!("this vault has no owner yet; use `create-owner` to claim it");
    }

    let hash = crate::auth::hash_password(password)?;
    account_profile::update_password_hash(&mut conn, account_profile::OWNER_ACCOUNT_ID, &hash)
        .await?;
    // The old password is gone, so every session it opened should be too.
    crate::db::session_tokens::revoke_account_sessions(
        &mut conn,
        account_profile::OWNER_ACCOUNT_ID,
    )
    .await?;

    let username =
        account_profile::username_for_account(&mut conn, account_profile::OWNER_ACCOUNT_ID)
            .await?
            .unwrap_or_else(|| account_profile::OWNER_ACCOUNT_ID.to_string());

    drop(conn);
    pool.close().await;
    Ok(username)
}
