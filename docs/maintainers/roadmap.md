# Development roadmap

This roadmap defines the product boundary for the initial Message Vault
release. It is a development guide, not a promise of release dates.

## V1: import, browse, organize

The initial release should support this complete path:

1. Create and log in to a local vault account (including Hanko / Access when
   configured).
2. Copy the account Import API token from Settings → Access.
3. Import messages and attachments with
   [Message Exporters](https://bitrealm-dev.github.io/message-exporters/)
   (`vault-push` / the Vault tab), or with the vault CLI `import` against a
   JSONL directory. Optionally pass a local VCF or contacts CSV to
   `vault-push`, `import --contacts`, or `import-contacts` so names can be
   filled from an address book; vault-side `contacts.csv` / `exclude.csv`
   sidecars are not used.
4. Browse direct conversations, group messages, contacts, attachments, and
   sources. Contacts are also created from imported participants
   (`name_hint` → `preferred_name` when empty).
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
