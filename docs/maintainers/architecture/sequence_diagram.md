# Sequence Diagram - Developer workflow

The host processes are regular participants:

- **Desktop App (Tauri)** — native window started by `cargo tauri dev`
- **Vite :5173** — dev server that serves live `web/` source
- **Vault :8080** — `message-vault-server` started by `./scripts/run-vault-dev.sh`

## Start the vault

1. Run the vault with: `./scripts/run-vault-dev.sh`

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant Desktop as Desktop App (Tauri)
    participant Vite as Vite :5173
    participant Vault as Vault :8080

    participant WebSrc as web/

    Dev->>Vault: ./scripts/run-vault-dev.sh
    Note over Vault: cargo run -- serve. Restart this process after server-crate edits.

    Dev->>Desktop: cargo tauri dev
    Desktop->>Vite: Starts (npm run dev)
    loop Each window open or refresh
        Desktop->>Vite: Loads UI
        Vite->>WebSrc: Serves live
        Vite-->>Desktop: SPA assets
    end
```

## Sign in

**Prerequisite**

1. Vault is running on `:8080`.
2. Desktop App is running. (Vite is serving the WebView on `:5173`.)

The developer types credentials in the SPA. Login is an Auth API call to the vault, not a sign-in to Tauri.

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant Desktop as Desktop App (Tauri)
    participant Vite as Vite :5173
    participant Vault as Vault :8080

    participant DB as SQLite (data/vault.db)

    Dev->>Desktop: Starts
    Dev->>Desktop: Enters credentials
    Desktop->>Vault: Forwards credentials (Auth, API)
    Vault->>DB: Reads account (rusqlite)
    Vault-->>Desktop: Session
```

## Import a backup

**Prerequisite**

1. Vault is running on `:8080`
2. Desktop App is running. (Vite is serving webview on `:5173`.)
3. User is signed in.

Messages and attachments are uploaded to the vault using `vault_push`.

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant Desktop as Desktop App (Tauri)
    participant Vite as Vite :5173
    participant Vault as Vault :8080

    participant DB as SQLite (data/vault.db)
    participant Disk as data/ attachments
    participant Backups as Phone backup files

    Dev->>Desktop: Extract a backup
    Desktop->>Backups: Reads files (Extract / Format)
    Desktop-->>Dev: JSONL on disk

    Dev->>Desktop: Import into the vault
    Desktop->>Vault: Import JSONL (Import, API)
    Vault->>DB: Writes messages
    Desktop->>Vault: Upload attachments (Assets, API)
    Vault->>Disk: Writes files
```

## Export from the vault

**Prerequisite**

1. Vault is running on `:8080`
2. Desktop App is running. (Vite is serving webview on `:5173`.)
3. User is signed in.

Messages and attachments are downloaded from the vault using `vault_pull`.

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant Desktop as Desktop App (Tauri)
    participant Vite as Vite :5173
    participant Vault as Vault :8080

    participant DB as SQLite (data/vault.db)
    participant Disk as data/ attachments
    participant Out as Chosen folder

    Dev->>Desktop: Export the vault
    Desktop->>Vault: Export messages (Browse / export, API)
    Vault->>DB: Reads messages (rusqlite)
    Vault-->>Desktop: Message pages
    Desktop->>Vault: Download attachments (Assets, API)
    Vault->>Disk: Reads files
    Vault-->>Desktop: Attachment bytes
    Desktop->>Out: Writes JSONL and attachments
```
