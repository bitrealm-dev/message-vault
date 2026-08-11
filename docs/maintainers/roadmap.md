# Development roadmap

This roadmap defines the product boundary for the initial Message Vault
release. It is a development guide, not a promise of release dates.

## V1: import, browse, organize

The initial release should support this complete path:

1. Create and log in to a local vault account (including Hanko / Access when
   configured).
2. Copy the account Import API token from Settings → Access.
3. Import messages and attachments with
   the desktop app
   (`vault-push` / the Vault tab), or with the vault CLI `import` against a
   JSONL directory. Optionally pass a local VCF or contacts CSV to
   `vault-push`, `import --contacts`, or `import-contacts` so names can be
   filled from an address book; vault-side `contacts.csv` / `exclude.csv`
   sidecars are not used.
4. Browse direct conversations, group messages, contacts, attachments, and
   sources. Contacts are also created from imported participants
   (`name_alias` → `preferred_name` when empty).
5. Search contacts and messages, including full-text body/subject matching via
   the vault FTS index.
6. Edit contact preferred names and phone numbers.
7. Create, rename, assign, and unassign labels, with undo and redo.

### Search

V1 includes full-text search over message bodies and related fields (the
`messages_fts` path). Metadata filters (dates, sources, labels, attachment
names/MIME types, participants) remain available alongside it.

### Contact and label editing

V1 includes:

- changing `preferred_name`;
- adding, replacing, or removing phone numbers (phone removal is an edit);
- creating and renaming labels;
- assigning or unassigning labels for one or many contacts;
- undo and redo for label creation, rename, membership, and clear-all labels.

Undo/redo history should survive a trip to Settings and back during the same
browser session.

### Settings navigation

Settings tabs include Account, Access, Storage, and Appearance.

Opening Settings must preserve the full browse location, including the selected
contact or conversation, submitted search, source, and applicable year filter.
Settings needs a visible return control, and switching Settings tabs must keep
the return destination.

### Deletion boundary

Deletion is not part of the V1 GUI:

- Hide Trash from the application navigation.
- Keep Delete actions visible where their menu position matters, but always
  disabled and greyed out.
- Disable Delete/Backspace shortcuts and destructive merge actions.
- Disable contact, message, group-message, label, account, and “delete all
  messages” controls.
- Keep the existing handlers, Trash route, and deletion APIs wired for V2.
- Do not block direct API deletion as part of the V1 work.

## V1 acceptance checks

Before calling V1 complete:

- Account creation and login work.
- `vault-push` authentication and import smoke tests pass.
- Imported conversations and attachments are browsable.
- Full-text search matches body text; metadata filters still work.
- Contact preferred-name and phone edits persist.
- Label create, rename, assign/unassign, undo, and redo work.
- Settings returns to the prior browse URL and preserves undo/redo history.
- Every destructive GUI action is disabled; Trash is absent from navigation.
- Rust tests, web tests/lint/build, and documentation checks pass.

## V2 candidates

The following are intentionally deferred:

- enabling Trash navigation;
- soft deletion and permanent deletion in the GUI;
- contact merge in the GUI;
- deletion-oriented undo/redo;
- broader lifecycle controls for deleting all messages or an account in the GUI.

V2 should reuse the APIs and client handlers retained behind the V1 GUI
capability gate (`web/src/lib/v1Capabilities.ts`).

## Future: VPS hosting and Hanko authentication

Message Vault should support a production VPS deployment with passwordless
Hanko authentication and one isolated vault account per Hanko user. Dev and
Access-settings Hanko wiring already exist on main; this section describes
production identity binding, host layout, and deploy automation still to
harden.

### Repository boundary

- Keep application code, the release image, Hanko integration, database
  migrations, and generic architecture documentation in this repository.
- Keep instance-specific deployment state in a separate private
  `message-vault-ops` repository: Docker Compose, Caddy routes, Hanko
  configuration templates, pinned image versions, deployment workflows, and
  operator runbooks.
- Keep secrets and persistent state outside both repositories under
  `/srv/message-vault/`, including the vault SQLite database and media, Hanko
  PostgreSQL data, and backups.

### Runtime architecture

The VPS stack should contain:

- Caddy for TLS and routing;
- the Message Vault release container, with the web UI on port 3000 and
  Rust import API on port 8080;
- the Hanko public API and a one-shot Hanko migration job;
- PostgreSQL for Hanko;
- an external SMTP provider for passwordless email delivery.

Use separate hostnames such as `vault.example.com`, `auth.example.com`, and
`import.example.com`. Bundle Hanko Elements into the web application.
Keep the import API's per-account bearer tokens independent from Hanko browser
sessions.

### Identity model

- Keep a unique mapping from each verified Hanko user ID to one Message Vault
  account ID (first-login provisioning already exists; harden for production).
- Prefer Hanko Elements and server-side Hanko session validation over any
  public account picker when `VAULT_AUTH=hanko`.
- Derive every browser request's account exclusively from the verified Hanko
  subject (not from a client-supplied account id).
- Provision the local account idempotently on first login, then collect any
  Message Vault profile fields not supplied by Hanko.
- Preserve the existing `account_id` isolation for messages, contacts,
  settings, media, and import tokens.

### Build, deployment, and maintenance

- Build and test `Dockerfile.release` in GitHub Actions and publish immutable
  commit-addressed images to GHCR.
- Pin Message Vault, Hanko, PostgreSQL, and Caddy image versions in the private
  `message-vault-ops` repository.
- Use Renovate to propose image updates. A merge to the ops repository's main
  branch should deploy over SSH, run Hanko migrations, pull images, apply
  Compose, and verify health endpoints.
- Provide idempotent VPS bootstrap, backup/restore, rollback, log inspection,
  and disaster-recovery procedures.
- Back up both the vault SQLite/media directory and Hanko PostgreSQL data on a
  systemd timer.

### Acceptance checks

- Two Hanko identities resolve to different vault accounts and cannot select
  each other's account.
- Invalid or expired Hanko sessions are rejected.
- First-login provisioning is idempotent.
- Import API tokens remain account-scoped.
- Compose validation, release image build, migrations, restart persistence,
  health checks, backup/restore, and image rollback all pass.
