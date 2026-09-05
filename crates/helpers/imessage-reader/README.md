# imessage-reader

Reads Apple Messages (a Mac `chat.db` or an iPhone backup folder) for the Message Vault desktop app, as a separate program.

The app starts this program, writes one JSON request on its stdin, and reads JSON events off its stdout: log lines, one record per conversation, one record per message, then a done line. For an encrypted backup the app then asks for attachments one at a time and the program decrypts each into a scratch folder the app owns. The protocol is [`imessage-reader-protocol`](../imessage-reader-protocol/). Nobody types this program at a shell; the installer places it beside the app and the app finds it there.

## Why a separate program

This crate links [`imessage-database`](https://crates.io/crates/imessage-database) and [`crabapple`](https://crates.io/crates/crabapple), which are GPL-3.0-or-later. Message Vault is under the Fair Core License, and the GPL does not allow the two to be distributed as one binary. A process boundary keeps the GPL on this side of it. The policy is [`docs/agents/licences.md`](../../../docs/agents/licences.md); the reasoning against ADR 0001's rule of no command line is in that ADR's amendment.

What lives here is the reading: opening the database, decrypting a backup, caching chats, handles, contacts and tapbacks, and classifying each row. Turning the records into the shared conversation structure, writing files, and media handling stay in [`imessage-ir-exporter`](../../exporters/imessage-ir-exporter/), which is FCL.

## Build and test

```bash
cargo build -p imessage-reader
cargo test -p imessage-reader
```

`src-tauri/build.rs` builds this crate and copies the binary to `src-tauri/binaries/` for Tauri to bundle, so `cargo tauri dev` and `cargo tauri build` need no separate step.

Workspace setup: [CONTRIBUTING.md](../../../CONTRIBUTING.md).

## License

GNU General Public License v3.0 or later. See [`LICENSE`](LICENSE) in this folder. This is the one crate in the repository not under the Fair Core License.
