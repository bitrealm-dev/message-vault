# iMessage Import Methods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Import screen’s two Apple sources with one **iMessage** source that has three extraction methods (Mac Messages, iPhone backup, Jailbroken iPhone), and pass the converter the path, password, attachment root, and Apple Contacts file each method actually uses.

**Architecture:** Keep the extract/staging ids `imessage-ios` and `imessage-macos`, and add `imessage-jailbreak`. The source dropdown shows a single **iMessage** row; a second dropdown picks the method. Jailbreak uses `ApplePlatform::MacOs` plus a required attachment root. Path existence, file-vs-folder kind, and “Manifest.plist says encrypted” are decided in pure TypeScript from stats returned by new Tauri commands. After-start extract errors a person can act on are rewritten in the converter (and iMessage ffmpeg check in `Form::to_imessage_config`) so CLI and the desktop summary share one sentence. The desktop extract command does not yet pass attachment root or Apple Contacts.

**Tech Stack:** React 19 + TypeScript in `web/`, Vitest + Testing Library, Tauri v2 commands in `src-tauri/`, `imessage-ir-exporter` + `message-vault-io-core` `Form` / `AppleConfig`.

**Spec:** `docs/superpowers/specs/2026-08-25-imessage-import-methods-design.md`

## Global Constraints

- Do not change WhatsApp, SMS Backup & Restore, or other non-Apple Import sources except where they share a control (the source list loses the two Apple rows).
- Do not show `--platform` as a control. Derive it: iPhone backup → `iOS`; Mac Messages and Jailbroken iPhone → `macOS`.
- Do not add date range, conversation filter, disk-space bypass, owner display-name flags, or `--use-message-times`.
- Do not show attachment folder or Apple Contacts on iPhone backup.
- Attachments Copy / Convert / Compress / Skip stay on the existing vault media pipeline. Do not add an ImageMagick-style copy-method picker.
- Vault contact merge (fill missing / overwrite / as-is) stays. It is not `--contacts-path`.
- Internal ids: Mac Messages `imessage-macos` (staging slug `macos`); iPhone backup `imessage-ios` (slug `iphone-ios`); Jailbroken iPhone `imessage-jailbreak` (slug `iphone-jailbreak`).
- Existing `imessage-ios` / `imessage-macos` remembered backup paths must keep working.
- Empty optional attachment-root and contacts-path values are omitted so Mac auto-scan and default attachment layout still run.
- The GUI must never prompt for a backup password on a terminal. Feed the password from the form field. If the backup is encrypted and the password is empty, fail with: `The backup is encrypted — fill Encryption password.`
- User-facing errors are the locked catalog in the spec section **User-facing errors**. Copy those sentences verbatim. Form errors sit under the field and keep Import disabled. After-start errors sit in the progress summary (same string on the CLI).
- Required path empty: no extra sentence. The label already says (required).
- `RuntimeError::InvalidOptions` Display is the sentence only. Do not prefix with `Invalid options!`.
- Leftover password on an unencrypted backup: `This backup is not encrypted. Clear Encryption password.` Do not keep `--cleartext-password was provided…`. Do not ignore a leftover password.
- After-start catalog (converter / iMessage `Form::to_imessage_config`): `Attachment folder does not exist.` / `Apple Contacts file does not exist.` / `Messages database does not exist.` / `This folder is not an iPhone backup, or Messages is missing from it.` / `The iOS backup password was incorrect.` / `Convert and Compress need ffmpeg and ffprobe. Put them on PATH, or in the desktop app set the ffmpeg directory in Settings → System.`
- Engine text stays for disk I/O, SQLite permission failures, crabapple parse failures other than “not a backup”, cancel, and `media processing failed for all candidate files`.
- Apple Contacts empty + auto-scan miss, and iPhone Contacts decrypt failure, stay log warnings. They are not a failed Import.
- Do not change the WhatsApp/SMS ffmpeg string in `Form::validate_media`. Only the iMessage `to_imessage_config` path uses the locked ffmpeg sentence.
- Default method on first open: iPhone backup (`imessage-ios`).
- Mac Messages pre-fill of `~/Library/Messages/chat.db` is macOS-only, and only when that file exists. Never pre-fill home-directory paths for iPhone backup, jailbreak, Linux, or Windows.
- Import is desktop-only (`isTauri()`). Playwright against Vite cannot exercise this screen. Prove behavior with Vitest and Rust tests. A later `cargo tauri dev` pass is optional, not a substitute for those tests.
- Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.
- Never commit to `main`. Work on `feat/imessage-import-methods`.
- Product version files stay at the current lockstep value. Do not bump versions.
- Follow `web/` Biome + existing Import form styling (`StackedField`, `Select`, `PathPicker`). Do not invent a new visual language.

## File map

| File | Responsibility |
|---|---|
| `web/src/lib/imessageImport.ts` | Method ids, labels, which fields show, Import-enable rules, path-kind errors, Mac default path |
| `web/src/lib/imessageExtractFields.ts` | Build the extract payload for the three iMessage methods (media always; password only on iOS; attachment root / contacts only on Mac and jailbreak, omitted when empty) |
| `web/src/lib/exportSources.ts` | One **iMessage** row in the source list; other sources unchanged |
| `web/src/lib/system-settings.ts` | Staging slug `iphone-jailbreak`; remembered extra paths per method |
| `web/src/lib/types.ts` / `web/src/lib/tauri.ts` | `attachment_root`, `apple_contacts` on extract; `path_stat` and `ios_backup_encrypted` wrappers |
| `web/src/screens/import/ImportFormFields.tsx` | Method dropdown and per-method fields |
| `web/src/screens/ImportScreen.tsx` | Method state, remembered extras, Mac pre-fill, live path probe |
| `web/src/screens/import/useImportJob.ts` | Pass media for all three methods; pass new extract fields |
| `src-tauri/src/commands/paths.rs` | `path_stat`; wrap encrypted-flag helper |
| `src-tauri/src/commands/extract.rs` | Accept jailbreak id, `attachment_root`, `apple_contacts`; map jailbreak to `ApplePlatform::MacOs` |
| `crates/exporters/imessage-ir-exporter/src/error.rs` | `InvalidOptions` Display is the sentence only; locked error constants |
| `crates/exporters/imessage-ir-exporter/src/backup.rs` | No stdin password prompt; encrypted-flag helper; leftover-password copy |
| `crates/exporters/imessage-ir-exporter/src/run.rs` | Missing attachment folder / Apple Contacts / messages db / not-an-iPhone-backup copy |
| `crates/core/message-vault-io-core/src/exporters.rs` | iMessage Convert/Compress ffmpeg sentence (not WhatsApp/SMS `validate_media`) |
| `docs/src/content/docs/vault/user/import-from-a-backup.md` | User-facing source/method table |
| `docs/src/content/docs/vault/user/prepare-a-backup/iphone-ipad.md` | Three ways to get Apple Messages |
| `CHANGELOG.md` | Unreleased notes dated 2026-08-26 |

Out of scope files: `crates/message-vault-io-gui/**`, `web-next/**`, WhatsApp form branches, vault server schema.

---

### Task 0: Branch and record the spec

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-26-imessage-import-methods.md` (already written)
- Add: `docs/superpowers/specs/2026-08-25-imessage-import-methods-design.md` (uncommitted)

**Interfaces:**
- Consumes: locked spec on disk
- Produces: git branch `feat/imessage-import-methods` with spec + plan committed

- [ ] **Step 1: Create the branch from up-to-date main**

```bash
cd /home/mbeisser/repo/message-vault
git fetch origin
git checkout main
git pull --ff-only origin main
git checkout -b feat/imessage-import-methods
git branch --show-current
```

Expected: `feat/imessage-import-methods`. Stop if this prints `main`.

- [ ] **Step 2: Commit the spec and this plan**

```bash
git add docs/superpowers/specs/2026-08-25-imessage-import-methods-design.md \
        docs/superpowers/plans/2026-08-26-imessage-import-methods.md
git commit -m "$(cat <<'EOF'
docs: add iMessage import methods spec and plan

Lock the three extraction methods, form fields, validation rules,
and user-facing error catalog before changing the Import screen.
EOF
)"
```

---

### Task 1: Method catalog and Import-enable rules

**Files:**
- Create: `web/src/lib/imessageImport.ts`
- Create: `web/src/lib/imessageImport.test.ts`

**Interfaces:**
- Consumes: spec tables for methods, fields, enable rules, path kind, defaults
- Produces:
  - `IMESSAGE_SOURCE_ID = "imessage"`
  - `IMESSAGE_DEFAULT_METHOD = "imessage-ios"`
  - `IMESSAGE_METHODS`: `{ id, label }[]` with ids `imessage-macos`, `imessage-ios`, `imessage-jailbreak`
  - `ImessageMethodId`
  - `isImessageMethod(source: string): source is ImessageMethodId`
  - `imessageApplePlatform(method: ImessageMethodId): "macOS" | "iOS"`
  - `imessageShowsPassword(method)` / `imessageShowsAttachmentRoot(method)` / `imessageShowsAppleContacts(method)` / `imessageAttachmentRootRequired(method)`
  - `PathStat = { exists: boolean; isFile: boolean; isDirectory: boolean }`
  - `ImessagePathStats = { backup: PathStat | null; attachmentRoot: PathStat | null; appleContacts: PathStat | null; backupEncrypted: boolean | null }`
  - `IMESSAGE_ERR_PATH_MISSING` / `IMESSAGE_ERR_IPHONE_PATH_IS_FILE` / `IMESSAGE_ERR_MAC_PATH_IS_DIR` / `IMESSAGE_ERR_JAILBREAK_PATH_IS_DIR` / `IMESSAGE_ERR_ATTACHMENT_IS_FILE` / `IMESSAGE_ERR_CONTACTS_IS_DIR` / `IMESSAGE_ERR_ENCRYPTED_PASSWORD` — exact spec catalog strings
  - `imessageCanImport(args): { enabled: boolean; errors: Partial<Record<"backupPath" | "attachmentRoot" | "appleContacts" | "backupPassword", string>> }`
  - `macMessagesDbPath(homeDir: string): string`
  - `shouldPrefillMacMessagesDb(args): string`

`backup: null` means the desktop backend has not answered yet. A non-empty path with `null` stats does **not** enable Import (fail before a long run).

- [ ] **Step 1: Write the failing tests**

Create `web/src/lib/imessageImport.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import {
  IMESSAGE_DEFAULT_METHOD,
  IMESSAGE_METHODS,
  IMESSAGE_SOURCE_ID,
  imessageApplePlatform,
  imessageCanImport,
  imessageShowsAppleContacts,
  imessageShowsAttachmentRoot,
  imessageShowsPassword,
  isImessageMethod,
  macMessagesDbPath,
  shouldPrefillMacMessagesDb,
  IMESSAGE_ERR_ATTACHMENT_IS_FILE,
  IMESSAGE_ERR_CONTACTS_IS_DIR,
  IMESSAGE_ERR_ENCRYPTED_PASSWORD,
  IMESSAGE_ERR_IPHONE_PATH_IS_FILE,
  IMESSAGE_ERR_JAILBREAK_PATH_IS_DIR,
  IMESSAGE_ERR_MAC_PATH_IS_DIR,
  IMESSAGE_ERR_PATH_MISSING,
} from "./imessageImport";

const presentFile: { exists: true; isFile: true; isDirectory: false } = {
  exists: true,
  isFile: true,
  isDirectory: false,
};
const presentDir: { exists: true; isFile: false; isDirectory: true } = {
  exists: true,
  isFile: false,
  isDirectory: true,
};
const missing: { exists: false; isFile: false; isDirectory: false } = {
  exists: false,
  isFile: false,
  isDirectory: false,
};

describe("iMessage methods", () => {
  it("lists three methods and defaults to iPhone backup", () => {
    expect(IMESSAGE_SOURCE_ID).toBe("imessage");
    expect(IMESSAGE_DEFAULT_METHOD).toBe("imessage-ios");
    expect(IMESSAGE_METHODS.map((m) => m.id)).toEqual([
      "imessage-macos",
      "imessage-ios",
      "imessage-jailbreak",
    ]);
    expect(IMESSAGE_METHODS.map((m) => m.label)).toEqual([
      "Mac Messages",
      "iPhone backup",
      "Jailbroken iPhone",
    ]);
  });

  it("derives converter platform from the method", () => {
    expect(imessageApplePlatform("imessage-ios")).toBe("iOS");
    expect(imessageApplePlatform("imessage-macos")).toBe("macOS");
    expect(imessageApplePlatform("imessage-jailbreak")).toBe("macOS");
  });

  it("shows password only for iPhone backup", () => {
    expect(imessageShowsPassword("imessage-ios")).toBe(true);
    expect(imessageShowsPassword("imessage-macos")).toBe(false);
    expect(imessageShowsPassword("imessage-jailbreak")).toBe(false);
  });

  it("shows attachment root and Apple Contacts on Mac and jailbreak only", () => {
    expect(imessageShowsAttachmentRoot("imessage-macos")).toBe(true);
    expect(imessageShowsAttachmentRoot("imessage-jailbreak")).toBe(true);
    expect(imessageShowsAttachmentRoot("imessage-ios")).toBe(false);
    expect(imessageShowsAppleContacts("imessage-macos")).toBe(true);
    expect(imessageShowsAppleContacts("imessage-jailbreak")).toBe(true);
    expect(imessageShowsAppleContacts("imessage-ios")).toBe(false);
  });

  it("treats only the three method ids as iMessage methods", () => {
    expect(isImessageMethod("imessage-ios")).toBe(true);
    expect(isImessageMethod("whatsapp-android")).toBe(false);
    expect(isImessageMethod("imessage")).toBe(false);
  });
});

describe("imessageCanImport", () => {
  it("enables iPhone backup when the folder exists", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/backups/iphone",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentDir,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: false,
      },
    });
    expect(result.enabled).toBe(true);
    expect(result.errors).toEqual({});
  });

  it("keeps password optional when encryption is unknown", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/backups/iphone",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentDir,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(true);
  });

  it("requires password when Manifest.plist is marked encrypted", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/backups/iphone",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentDir,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: true,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.backupPassword).toBe(IMESSAGE_ERR_ENCRYPTED_PASSWORD);
  });

  it("enables an encrypted backup when the password is filled", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/backups/iphone",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "secret",
      stats: {
        backup: presentDir,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: true,
      },
    });
    expect(result.enabled).toBe(true);
  });

  it("rejects an iPhone backup path that is a .db file", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/copy/sms.db",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.backupPath).toBe(IMESSAGE_ERR_IPHONE_PATH_IS_FILE);
  });

  it("enables Mac Messages when chat.db exists", () => {
    const result = imessageCanImport({
      method: "imessage-macos",
      backupPath: "/Users/sam/Library/Messages/chat.db",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(true);
  });

  it("rejects Mac or jailbreak when the path is a directory", () => {
    const mac = imessageCanImport({
      method: "imessage-macos",
      backupPath: "/Users/sam/Library/Messages",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentDir,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(mac.enabled).toBe(false);
    expect(mac.errors.backupPath).toBe(IMESSAGE_ERR_MAC_PATH_IS_DIR);

    const jail = imessageCanImport({
      method: "imessage-jailbreak",
      backupPath: "/mnt/iphone/Library/SMS",
      attachmentRoot: "/mnt/iphone/Library/SMS",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentDir,
        attachmentRoot: presentDir,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(jail.enabled).toBe(false);
    expect(jail.errors.backupPath).toBe(IMESSAGE_ERR_JAILBREAK_PATH_IS_DIR);
  });

  it("requires jailbreak sms.db and attachment folder", () => {
    const missingRoot = imessageCanImport({
      method: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(missingRoot.enabled).toBe(false);

    const ready = imessageCanImport({
      method: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "/mnt/iphone/Library/SMS",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: presentDir,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(ready.enabled).toBe(true);
  });

  it("disables Import when an optional extra path is set but missing", () => {
    const result = imessageCanImport({
      method: "imessage-macos",
      backupPath: "/tmp/chat.db",
      attachmentRoot: "/tmp/missing-attachments",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: missing,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.attachmentRoot).toBe(IMESSAGE_ERR_PATH_MISSING);
  });

  it("rejects an attachment folder that is a file", () => {
    const result = imessageCanImport({
      method: "imessage-macos",
      backupPath: "/tmp/chat.db",
      attachmentRoot: "/tmp/chat.db",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: presentFile,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.attachmentRoot).toBe(IMESSAGE_ERR_ATTACHMENT_IS_FILE);
  });

  it("rejects an Apple Contacts path that is a directory", () => {
    const result = imessageCanImport({
      method: "imessage-macos",
      backupPath: "/tmp/chat.db",
      attachmentRoot: "",
      appleContacts: "/tmp/AddressBook",
      backupPassword: "",
      stats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: presentDir,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.appleContacts).toBe(IMESSAGE_ERR_CONTACTS_IS_DIR);
  });

  it("disables Import while a non-empty path has not been checked yet", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "/backups/iphone",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: null,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
  });

  it("disables Import when the backup path is empty", () => {
    const result = imessageCanImport({
      method: "imessage-ios",
      backupPath: "  ",
      attachmentRoot: "",
      appleContacts: "",
      backupPassword: "",
      stats: {
        backup: null,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(result.enabled).toBe(false);
  });
});

describe("Mac Messages pre-fill", () => {
  it("joins chat.db under the home Library folder", () => {
    expect(macMessagesDbPath("/Users/sam")).toBe("/Users/sam/Library/Messages/chat.db");
    expect(macMessagesDbPath("/Users/sam/")).toBe("/Users/sam/Library/Messages/chat.db");
  });

  it("pre-fills only on macOS when the file exists and nothing is remembered", () => {
    expect(
      shouldPrefillMacMessagesDb({
        os: "macos",
        homeDir: "/Users/sam",
        chatDbExists: true,
        rememberedPath: "",
      }),
    ).toBe("/Users/sam/Library/Messages/chat.db");
    expect(
      shouldPrefillMacMessagesDb({
        os: "linux",
        homeDir: "/home/sam",
        chatDbExists: true,
        rememberedPath: "",
      }),
    ).toBe("");
    expect(
      shouldPrefillMacMessagesDb({
        os: "macos",
        homeDir: "/Users/sam",
        chatDbExists: false,
        rememberedPath: "",
      }),
    ).toBe("");
    expect(
      shouldPrefillMacMessagesDb({
        os: "macos",
        homeDir: "/Users/sam",
        chatDbExists: true,
        rememberedPath: "/copied/chat.db",
      }),
    ).toBe("/copied/chat.db");
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd web && npx vitest run src/lib/imessageImport.test.ts`

Expected: FAIL because `imessageImport.ts` does not exist.

- [ ] **Step 3: Implement the module**

Create `web/src/lib/imessageImport.ts` with the exports the tests import. Put the catalog sentences on named constants (exact spec copy):

```typescript
export const IMESSAGE_ERR_PATH_MISSING = "This path does not exist.";
export const IMESSAGE_ERR_IPHONE_PATH_IS_FILE =
  "Pick the backup folder, or switch to Jailbroken iPhone.";
export const IMESSAGE_ERR_MAC_PATH_IS_DIR = "Pick chat.db.";
export const IMESSAGE_ERR_JAILBREAK_PATH_IS_DIR = "Pick sms.db.";
export const IMESSAGE_ERR_ATTACHMENT_IS_FILE =
  "Pick the folder that contains Attachments and StickerCache.";
export const IMESSAGE_ERR_CONTACTS_IS_DIR =
  "Pick AddressBook-v22.abcddb or AddressBook.sqlitedb.";
export const IMESSAGE_ERR_ENCRYPTED_PASSWORD =
  "The backup is encrypted — fill Encryption password.";
```

Exact enable logic:

1. Trim `backupPath`. Empty → `{ enabled: false, errors: {} }` (no extra sentence).
2. If `stats.backup` is `null` → `{ enabled: false, errors: {} }`.
3. If `!stats.backup.exists` → error `backupPath`: `IMESSAGE_ERR_PATH_MISSING`.
4. Path kind:
   - `imessage-ios` and `stats.backup.isFile` → `IMESSAGE_ERR_IPHONE_PATH_IS_FILE`
   - `imessage-macos` and `stats.backup.isDirectory` → `IMESSAGE_ERR_MAC_PATH_IS_DIR`
   - `imessage-jailbreak` and `stats.backup.isDirectory` → `IMESSAGE_ERR_JAILBREAK_PATH_IS_DIR`
5. Jailbreak: trimmed `attachmentRoot` empty → `enabled: false` (no extra error). If non-empty, `stats.attachmentRoot` must be non-null. Then:
   - `!exists` → `IMESSAGE_ERR_PATH_MISSING`
   - `isFile` (not a directory) → `IMESSAGE_ERR_ATTACHMENT_IS_FILE`
   - otherwise must be `isDirectory`
6. Mac: empty attachment root is fine. If non-empty, same existence and kind checks as jailbreak.
7. Apple Contacts: empty is fine. If non-empty, `stats.appleContacts` must be non-null. Then:
   - `!exists` → `IMESSAGE_ERR_PATH_MISSING`
   - `isDirectory` → `IMESSAGE_ERR_CONTACTS_IS_DIR`
   - otherwise must be `isFile`
8. `imessage-ios` and `stats.backupEncrypted === true` and trimmed password empty → error `backupPassword`: `IMESSAGE_ERR_ENCRYPTED_PASSWORD`
9. `enabled` is true only when `errors` is empty and the required paths are non-empty.

`imessageApplePlatform`: `"imessage-ios"` → `"iOS"`, otherwise `"macOS"`.

`macMessagesDbPath`: strip trailing `/` or `\\` from `homeDir`, then append `/Library/Messages/chat.db`. Empty home → `""`.

`shouldPrefillMacMessagesDb`: if `rememberedPath.trim()` return that; if `os !== "macos"` or `!chatDbExists` return `""`; else `macMessagesDbPath(homeDir)`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd web && npx vitest run src/lib/imessageImport.test.ts`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/imessageImport.ts web/src/lib/imessageImport.test.ts
git commit -m "$(cat <<'EOF'
feat(import): add iMessage method rules

Decide which fields each Apple extraction method needs, and when
Import may start, before changing the form.
EOF
)"
```

---

### Task 2: Remembered extra paths and jailbreak staging slug

**Files:**
- Modify: `web/src/lib/system-settings.ts` (`importerSlugForSource`, new extra-path helpers)
- Modify: `web/src/lib/system-settings.test.ts`

**Interfaces:**
- Consumes: method ids from Task 1 (`imessage-jailbreak`)
- Produces:
  - `importerSlugForSource("imessage-jailbreak") === "iphone-jailbreak"`
  - `getImporterExtraPaths(sourceId: string): { attachmentRoot: string; appleContacts: string }`
  - `setImporterExtraPath(sourceId: string, field: "attachmentRoot" | "appleContacts", path: string): void`
  - Legacy `getImporterPath` / `setImporterPath` still read `mv-importer-paths` as `Record<string, string>`

Store extras in a new localStorage key `mv-importer-extra-paths` as `Record<string, { attachmentRoot?: string; appleContacts?: string }>`. Do not reuse the backup-path map for these fields. Switching methods must not copy a `chat.db` path into the backup-folder field (already true if extras are keyed by method id).

- [ ] **Step 1: Write the failing tests**

Add to `web/src/lib/system-settings.test.ts` (the file already mocks `localStorage` with a `Map`):

```typescript
import {
  getImporterExtraPaths,
  getImporterPath,
  joinImportStagingPath,
  setImporterExtraPath,
  setImporterPath,
} from "./system-settings";

describe("joinImportStagingPath jailbreak slug", () => {
  const now = new Date(2026, 7, 24, 18, 5, 9);

  it("uses iphone-jailbreak in the staging folder name", () => {
    expect(joinImportStagingPath("/home/sam/message-vault", "imessage-jailbreak", now)).toBe(
      "/home/sam/message-vault/staging-iphone-jailbreak-260824-180509",
    );
  });
});

describe("remembered importer extra paths", () => {
  it("keeps a legacy backup path string for imessage-ios", () => {
    setImporterPath("imessage-ios", "/backups/old-iphone");
    expect(getImporterPath("imessage-ios")).toBe("/backups/old-iphone");
    expect(getImporterExtraPaths("imessage-ios")).toEqual({
      attachmentRoot: "",
      appleContacts: "",
    });
  });

  it("stores attachment folder and Apple Contacts per method", () => {
    setImporterPath("imessage-macos", "/Users/sam/Library/Messages/chat.db");
    setImporterExtraPath("imessage-macos", "attachmentRoot", "/Users/sam/Library/Messages");
    setImporterExtraPath(
      "imessage-macos",
      "appleContacts",
      "/Users/sam/Library/Application Support/AddressBook/AddressBook-v22.abcddb",
    );
    setImporterPath("imessage-jailbreak", "/mnt/iphone/sms.db");
    setImporterExtraPath("imessage-jailbreak", "attachmentRoot", "/mnt/iphone/Library/SMS");

    expect(getImporterPath("imessage-macos")).toBe("/Users/sam/Library/Messages/chat.db");
    expect(getImporterExtraPaths("imessage-macos")).toEqual({
      attachmentRoot: "/Users/sam/Library/Messages",
      appleContacts:
        "/Users/sam/Library/Application Support/AddressBook/AddressBook-v22.abcddb",
    });
    expect(getImporterPath("imessage-jailbreak")).toBe("/mnt/iphone/sms.db");
    expect(getImporterExtraPaths("imessage-jailbreak").attachmentRoot).toBe(
      "/mnt/iphone/Library/SMS",
    );
    expect(getImporterExtraPaths("imessage-macos").attachmentRoot).not.toBe(
      getImporterExtraPaths("imessage-jailbreak").attachmentRoot,
    );
  });

  it("clears an extra path when set to blank", () => {
    setImporterExtraPath("imessage-macos", "attachmentRoot", "/tmp/root");
    setImporterExtraPath("imessage-macos", "attachmentRoot", "  ");
    expect(getImporterExtraPaths("imessage-macos").attachmentRoot).toBe("");
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd web && npx vitest run src/lib/system-settings.test.ts`

Expected: FAIL on `getImporterExtraPaths` not defined and jailbreak slug falling through to `sourceId` (`staging-imessage-jailbreak-…`).

- [ ] **Step 3: Implement**

In `web/src/lib/system-settings.ts`, change `importerSlugForSource`:

```typescript
function importerSlugForSource(sourceId: string): string {
  if (sourceId === "imessage-ios") return "iphone-ios";
  if (sourceId === "imessage-macos") return "macos";
  if (sourceId === "imessage-jailbreak") return "iphone-jailbreak";
  return sourceId;
}
```

Add extra-path helpers next to `setImporterPath`. Use key `mv-importer-extra-paths`. Parse defensively (ignore non-objects). `setImporterExtraPath` with a blank trimmed value deletes that field; if a method object has no remaining fields, drop the method key.

Export:

```typescript
export type ImporterExtraField = "attachmentRoot" | "appleContacts";

export function getImporterExtraPaths(sourceId: string): {
  attachmentRoot: string;
  appleContacts: string;
} {
  const row = readImporterExtraPaths()[sourceId];
  return {
    attachmentRoot: row?.attachmentRoot ?? "",
    appleContacts: row?.appleContacts ?? "",
  };
}

export function setImporterExtraPath(
  sourceId: string,
  field: ImporterExtraField,
  path: string,
): void {
  // trim; write or delete as described above
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd web && npx vitest run src/lib/system-settings.test.ts`

Expected: PASS (existing staging tests still pass).

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/system-settings.ts web/src/lib/system-settings.test.ts
git commit -m "$(cat <<'EOF'
feat(import): remember iMessage extra paths per method

Keep backup folders, attachment roots, and AddressBook files from
overwriting each other when the extraction method changes.
EOF
)"
```

---

### Task 3: One iMessage source row and per-method form fields

**Files:**
- Modify: `web/src/lib/exportSources.ts`
- Modify: `web/src/lib/exportSources.test.ts`
- Modify: `web/src/screens/import/ImportFormFields.tsx`
- Create: `web/src/screens/import/ImportFormFields.test.tsx`
- Modify: `web/src/components/PathPicker.tsx` (optional `filters` for `.db` files)

**Interfaces:**
- Consumes: `IMESSAGE_SOURCE_ID`, `IMESSAGE_METHODS`, `isImessageMethod`, `imessageCanImport`, field-visibility helpers from Task 1
- Produces: source list id `imessage`; `ImportFormFields` extra props:
  - `attachmentRoot: string`
  - `onAttachmentRootChange: (path: string) => void`
  - `appleContacts: string`
  - `onAppleContactsChange: (path: string) => void`
  - `pathStats: ImessagePathStats`
  - `onSourceListChange` stays `onSourceChange` for non-iMessage ids; when the list key is `imessage`, the parent still holds a method id in `source`

When `isImessageMethod(props.source)`, the source `Select` `selectedKey` is `IMESSAGE_SOURCE_ID`. A second `Select` (`aria-label="Extraction method"`) uses `props.source` as `selectedKey`.

Do not show a `--platform` control.

Hints (verbatim):

- Attachment folder: `Folder that contains Attachments and StickerCache. Needed when those folders are not next to chat.db.`
- Apple Contacts, Mac: `AddressBook-v22.abcddb or AddressBook.sqlitedb. On a live Mac, empty means scan the local AddressBook.`
- Apple Contacts, jailbreak: `AddressBook-v22.abcddb or AddressBook.sqlitedb. A local Mac AddressBook scan will not find a phone copy.`

Path picker kinds:

- Mac: file picker, placeholder `Path to chat.db`, label `Messages database`
- iPhone backup: folder picker (existing `iPhone Backup Directory`)
- Jailbreak: file picker, placeholder `Path to sms.db`, label `Messages database`

Show Attachments (including compress extras) for all three iMessage methods and for SMS Backup & Restore. Show vault Contacts for all three iMessage methods.

`canImport` for iMessage methods is `imessageCanImport(...).enabled && !props.running`. Keep the existing SBR phone-mismatch logic. For other sources, keep `Boolean(props.backupPath) && !props.running`.

Render `errors.backupPath` / `attachmentRoot` / `appleContacts` / `backupPassword` under the matching field (`className={hintStyle}` plus `role="status"` when it is an error).

Password label: `Encryption password` when `pathStats.backupEncrypted === true`, otherwise `Encryption password (optional)`.

- [ ] **Step 1: Write the failing source-list test**

Replace the body of `web/src/lib/exportSources.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { EXPORT_SOURCES } from "./exportSources";
import { IMESSAGE_SOURCE_ID } from "./imessageImport";

describe("EXPORT_SOURCES", () => {
  it("lists one iMessage row instead of separate iOS and macOS sources", () => {
    const ids = EXPORT_SOURCES.map((s) => s.id);
    expect(ids).toContain(IMESSAGE_SOURCE_ID);
    expect(ids).not.toContain("imessage-ios");
    expect(ids).not.toContain("imessage-macos");
    expect(ids).toContain("whatsapp-android");
    expect(ids).toContain("sms-backup-restore");
    expect(EXPORT_SOURCES.find((s) => s.id === IMESSAGE_SOURCE_ID)?.label).toBe("iMessage");
    expect(new Set(ids).size).toBe(ids.length);
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cd web && npx vitest run src/lib/exportSources.test.ts`

Expected: FAIL (`imessage-ios` still in the list).

- [ ] **Step 3: Change the source list**

`web/src/lib/exportSources.ts`:

```typescript
import { IMESSAGE_SOURCE_ID } from "./imessageImport";

/** Backup sources offered by Import in the desktop app. */
export const EXPORT_SOURCES: { id: string; label: string }[] = [
  { id: IMESSAGE_SOURCE_ID, label: "iMessage" },
  { id: "whatsapp-android", label: "WhatsApp - Android" },
  { id: "whatsapp-ios", label: "WhatsApp - iOS" },
  { id: "sms-backup-restore", label: "SMS Backup & Restore" },
  { id: "go-sms-pro", label: "GO SMS Pro" },
  { id: "imazing", label: "iMazing" },
  { id: "sms-backup-plus", label: "SMS Backup+" },
  { id: "openextract", label: "OpenExtract" },
];
```

- [ ] **Step 4: Write ImportFormFields tests**

Create `web/src/screens/import/ImportFormFields.test.tsx`:

```tsx
/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ImportFormFields, { type ImportFormFieldsProps } from "./ImportFormFields";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

afterEach(() => {
  cleanup();
});

const presentFile = { exists: true, isFile: true, isDirectory: false };
const presentDir = { exists: true, isFile: false, isDirectory: true };

function renderForm(override: Partial<ImportFormFieldsProps> = {}) {
  const props: ImportFormFieldsProps = {
    source: "imessage-ios",
    onSourceChange: vi.fn(),
    backupPath: "/backups/iphone",
    onBackupPathChange: vi.fn(),
    backupPassword: "",
    onBackupPasswordChange: vi.fn(),
    showBackupPassword: false,
    onToggleBackupPassword: vi.fn(),
    attachmentRoot: "",
    onAttachmentRootChange: vi.fn(),
    appleContacts: "",
    onAppleContactsChange: vi.fn(),
    pathStats: {
      backup: presentDir,
      attachmentRoot: null,
      appleContacts: null,
      backupEncrypted: false,
    },
    attachmentMedia: "copy",
    onAttachmentMediaChange: vi.fn(),
    maxResolution: "720p",
    onMaxResolutionChange: vi.fn(),
    maxFps: "30",
    onMaxFpsChange: vi.fn(),
    minSizeMb: "20",
    onMinSizeMbChange: vi.fn(),
    contactNameMode: "fill_missing",
    onContactNameModeChange: vi.fn(),
    ownerPhones: [],
    onOwnerPhonesChange: vi.fn(),
    profilePhones: [],
    profilePhonesReady: true,
    profilePhonesError: false,
    showMissingAccountPhoneWarning: false,
    formatOpen: true,
    onToggleFormat: vi.fn(),
    processingOpen: false,
    onToggleProcessing: vi.fn(),
    force: false,
    onForceChange: vi.fn(),
    obfuscate: false,
    onObfuscateChange: vi.fn(),
    running: false,
    onImport: vi.fn(),
    ...override,
  };
  return render(<ImportFormFields {...props} />);
}

describe("ImportFormFields iMessage methods", () => {
  it("shows one iMessage source and an extraction method dropdown", () => {
    renderForm();
    expect(screen.getByLabelText("Import source")).toBeTruthy();
    expect(screen.getByLabelText("Extraction method")).toBeTruthy();
    expect(screen.queryByText("iPhone - iOS")).toBeNull();
    expect(screen.queryByText("iMessage - macOS")).toBeNull();
  });

  it("shows password and hides attachment folder on iPhone backup", () => {
    renderForm({ source: "imessage-ios" });
    expect(screen.getByLabelText("Encryption password")).toBeTruthy();
    expect(screen.queryByLabelText("Attachment folder")).toBeNull();
    expect(screen.queryByLabelText("Apple Contacts file")).toBeNull();
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("shows optional attachment folder on Mac Messages", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/Users/sam/Library/Messages/chat.db",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.queryByLabelText("Encryption password")).toBeNull();
    expect(screen.getByLabelText("Attachment folder")).toBeTruthy();
    expect(screen.getByLabelText("Apple Contacts file")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("disables Import on jailbreak until the attachment folder is set", () => {
    renderForm({
      source: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });

  it("enables jailbreak Import when sms.db and attachment folder exist", () => {
    renderForm({
      source: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "/mnt/iphone/Library/SMS",
      pathStats: {
        backup: presentFile,
        attachmentRoot: presentDir,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("shows an attachment-folder kind error when the path is a file", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/tmp/chat.db",
      attachmentRoot: "/tmp/chat.db",
      pathStats: {
        backup: presentFile,
        attachmentRoot: presentFile,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(
      screen.getByText("Pick the folder that contains Attachments and StickerCache."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });

  it("shows an Apple Contacts kind error when the path is a directory", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/tmp/chat.db",
      appleContacts: "/tmp/AddressBook",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: presentDir,
        backupEncrypted: null,
      },
    });
    expect(
      screen.getByText("Pick AddressBook-v22.abcddb or AddressBook.sqlitedb."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });
});
```

If React Aria’s `Select` does not expose `getByLabelText("Import source")` (the `aria-label` is on `Select`), use `screen.getByRole("button", { name: /iMessage/i })` instead and adjust the test to what the trigger actually shows. Do not skip the test.

- [ ] **Step 5: Run form tests and confirm they fail**

Run: `cd web && npx vitest run src/screens/import/ImportFormFields.test.tsx`

Expected: FAIL (missing props / no extraction method dropdown).

- [ ] **Step 6: Implement the form**

Add optional `filters` to `PathPicker`:

```typescript
filters?: { name: string; extensions: string[] }[];
```

Pass `filters` into `open({ multiple: false, filters: props.filters })` for file picks. Folder picks stay `open({ directory: true, multiple: false })`.

Database file filters: `[{ name: "SQLite database", extensions: ["db"] }]`.

Apple Contacts filters: `[{ name: "Apple AddressBook", extensions: ["abcddb", "sqlitedb"] }]`.

In `ImportFormFields.tsx`:

1. Extend `ImportFormFieldsProps` with the new fields from **Interfaces**.
2. Import helpers from `imessageImport.ts` and `IMESSAGE_SOURCE_ID` / `IMESSAGE_METHODS` / `EXPORT_SOURCES`.
3. Source `Select`: `selectedKey={isImessageMethod(props.source) ? IMESSAGE_SOURCE_ID : props.source}`. On change: if the key is `IMESSAGE_SOURCE_ID`, call `props.onSourceChange(isImessageMethod(props.source) ? props.source : "imessage-ios")`; otherwise `props.onSourceChange(String(k))`.
4. When `isImessageMethod(props.source)`, render the extraction method `Select` bound to `props.source` / `IMESSAGE_METHODS`.
5. Replace the current `isIos ? … : isSbr ? … : generic` tree with: iMessage branch (fields from the spec table) / SBR branch (unchanged) / generic folder picker.
6. `showCompress = (isImessageMethod(props.source) || isSbr) && props.attachmentMedia === "compress"`.
7. Obfuscate checkbox: keep `isIos || isSbr` where `isIos` means `props.source === "imessage-ios"` (do not add obfuscate to Mac/jailbreak).
8. Import enabled: iMessage uses `imessageCanImport`; SBR unchanged; others `Boolean(props.backupPath) && !props.running`.

`ImportScreen.tsx` will not compile until Task 7. To keep TypeScript green after this task, add the new props in `ImportScreen.tsx` as empty-state stubs (`attachmentRoot=""`, `pathStats` with `backup: null`, no-op setters) so `npx tsc --noEmit` in `web/` still typechecks. Those stubs are replaced in Task 7.

- [ ] **Step 7: Run the tests and confirm they pass**

```bash
cd web && npx vitest run src/lib/exportSources.test.ts src/screens/import/ImportFormFields.test.tsx src/lib/imessageImport.test.ts
cd web && npx biome check src/lib/exportSources.ts src/lib/imessageImport.ts src/screens/import/ImportFormFields.tsx src/components/PathPicker.tsx src/screens/ImportScreen.tsx
```

Expected: PASS / Biome clean.

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/exportSources.ts web/src/lib/exportSources.test.ts \
        web/src/screens/import/ImportFormFields.tsx web/src/screens/import/ImportFormFields.test.tsx \
        web/src/components/PathPicker.tsx web/src/screens/ImportScreen.tsx
git commit -m "$(cat <<'EOF'
feat(import): show one iMessage source with three methods

Put Mac, iPhone backup, and jailbreak behind a method dropdown so
the form only asks for the paths that method uses.
EOF
)"
```

---

### Task 4: Locked extract errors and no stdin password prompt

**Files:**
- Modify: `crates/exporters/imessage-ir-exporter/src/error.rs` (`InvalidOptions` Display; catalog constants)
- Modify: `crates/exporters/imessage-ir-exporter/src/backup.rs`
- Modify: `crates/exporters/imessage-ir-exporter/src/run.rs` (missing paths / not-an-iPhone-backup)
- Modify: `crates/exporters/imessage-ir-exporter/src/lib.rs` (re-export helper + constants used by Tauri)
- Modify: `crates/exporters/imessage-ir-exporter/Cargo.toml` (add `plist = "1.9.0"`; remove `rpassword`)
- Modify: `crates/exporters/imessage-ir-exporter/src/cli.rs` (help text for `--backup-password`)
- Modify: `crates/core/message-vault-io-core/src/exporters.rs` (iMessage ffmpeg sentence only)
- Regenerated: `docs/src/content/docs/vault/developer/reference/cli/imessage-ir-exporter.md` via `dump-cli-docs`

**Interfaces:**
- Consumes: `decrypt_backup` today calls `prompt_for_password()`; `RuntimeError::InvalidOptions` Display is `Invalid options!\n{why}`; leftover password is `--cleartext-password was provided…`; missing attachment/contacts use `Supplied … does not exist!`
- Produces:
  - Catalog constants (exact spec copy) on `error.rs`, re-exported as needed:
    - `ENCRYPTED_BACKUP_PASSWORD_REQUIRED` = `The backup is encrypted — fill Encryption password.`
    - `UNENCRYPTED_BACKUP_CLEAR_PASSWORD` = `This backup is not encrypted. Clear Encryption password.`
    - `IOS_BACKUP_PASSWORD_INCORRECT` = `The iOS backup password was incorrect.` (already this sentence; keep it)
    - `ATTACHMENT_FOLDER_MISSING` = `Attachment folder does not exist.`
    - `APPLE_CONTACTS_MISSING` = `Apple Contacts file does not exist.`
    - `MESSAGES_DATABASE_MISSING` = `Messages database does not exist.`
    - `NOT_AN_IPHONE_BACKUP` = `This folder is not an iPhone backup, or Messages is missing from it.`
  - `pub fn ios_backup_encrypted_flag(backup_root: &Path) -> Option<bool>`
  - `fn reject_leftover_password(is_encrypted: bool, provided: Option<&str>) -> Result<(), RuntimeError>` used by `decrypt_backup` when the backup is not encrypted
  - `password_for_encrypted_backup(provided: Option<&str>) -> Result<String, RuntimeError>` used by `decrypt_backup` instead of a TTY prompt
  - `RuntimeError::InvalidOptions` Display is `{why}` only (no `Invalid options!` prefix)
  - `rpassword` gone
  - `message_vault_io_core::CONVERT_COMPRESS_FFMPEG_REQUIRED` used only by `Form::to_imessage_config`

`ios_backup_encrypted_flag` must **not** require a full crabapple `ManifestData` parse (encrypted plists need `BackupKeyBag`). Read `Manifest.plist` with the `plist` crate and return:

- `None` if the file is missing, unreadable, or not a dictionary
- `Some(true)` / `Some(false)` from the `IsEncrypted` boolean
- `Some(false)` if the key is absent (same as crabapple’s `unwrap_or(false)`)

Do **not** rewrite disk I/O, SQLite permission failures, crabapple parse failures other than “not a backup”, cancel, or `media processing failed for all candidate files`. Those stay engine text. After the Display change they also have no `Invalid options!` prefix; that is intended.

- [ ] **Step 1: Write the failing tests in `backup.rs` and `error.rs`**

At the bottom of `crates/exporters/imessage-ir-exporter/src/backup.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{
        ios_backup_encrypted_flag, password_for_encrypted_backup, reject_leftover_password,
    };
    use crate::error::{ENCRYPTED_BACKUP_PASSWORD_REQUIRED, UNENCRYPTED_BACKUP_CLEAR_PASSWORD};
    use std::fs;

    fn write_plist(dir: &std::path::Path, is_encrypted: &str) {
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>IsEncrypted</key>
  <{is_encrypted}/>
</dict>
</plist>
"#
        );
        fs::write(dir.join("Manifest.plist"), body).unwrap();
    }

    #[test]
    fn encrypted_flag_none_when_manifest_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ios_backup_encrypted_flag(dir.path()), None);
    }

    #[test]
    fn encrypted_flag_none_when_manifest_is_garbage() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Manifest.plist"), b"not a plist").unwrap();
        assert_eq!(ios_backup_encrypted_flag(dir.path()), None);
    }

    #[test]
    fn encrypted_flag_reads_is_encrypted_boolean() {
        let encrypted = tempfile::tempdir().unwrap();
        write_plist(encrypted.path(), "true");
        assert_eq!(ios_backup_encrypted_flag(encrypted.path()), Some(true));

        let plain = tempfile::tempdir().unwrap();
        write_plist(plain.path(), "false");
        assert_eq!(ios_backup_encrypted_flag(plain.path()), Some(false));
    }

    #[test]
    fn missing_password_does_not_prompt() {
        let err = password_for_encrypted_backup(None).unwrap_err();
        assert_eq!(err.to_string(), ENCRYPTED_BACKUP_PASSWORD_REQUIRED);
        assert!(!err.to_string().contains("Invalid options"));
        assert!(password_for_encrypted_backup(Some("secret")).is_ok());
    }

    #[test]
    fn leftover_password_on_unencrypted_uses_locked_copy() {
        let err = reject_leftover_password(false, Some("secret")).unwrap_err();
        assert_eq!(err.to_string(), UNENCRYPTED_BACKUP_CLEAR_PASSWORD);
        assert!(reject_leftover_password(false, None).is_ok());
        assert!(reject_leftover_password(true, Some("secret")).is_ok());
    }
}
```

In `error.rs` `#[cfg(test)]`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_options_display_is_the_sentence() {
        let err = RuntimeError::InvalidOptions(
            ENCRYPTED_BACKUP_PASSWORD_REQUIRED.to_string(),
        );
        assert_eq!(err.to_string(), ENCRYPTED_BACKUP_PASSWORD_REQUIRED);
        assert!(!err.to_string().contains("Invalid options"));
    }
}
```

If `RuntimeError` is `pub(crate)`, the Display test in `error.rs` still compiles. Do not weaken the leftover-password assertion to a substring of the old `--cleartext-password` sentence.

- [ ] **Step 2: Run those tests and confirm they fail**

Run: `cargo test -p imessage-ir-exporter --lib encrypted_flag missing_password leftover_password invalid_options_display`

Expected: FAIL (functions/constants not defined, leftover still the old CLI sentence, Display still has `Invalid options!`).

- [ ] **Step 3: Implement prompt removal, Display, leftover password, encrypted flag**

Add `plist = "1.9.0"` to `crates/exporters/imessage-ir-exporter/Cargo.toml`. Remove the `rpassword` dependency.

In `error.rs`, add (exact spec copy):

```rust
pub const ENCRYPTED_BACKUP_PASSWORD_REQUIRED: &str =
    "The backup is encrypted — fill Encryption password.";
pub const UNENCRYPTED_BACKUP_CLEAR_PASSWORD: &str =
    "This backup is not encrypted. Clear Encryption password.";
pub const IOS_BACKUP_PASSWORD_INCORRECT: &str = "The iOS backup password was incorrect.";
pub const ATTACHMENT_FOLDER_MISSING: &str = "Attachment folder does not exist.";
pub const APPLE_CONTACTS_MISSING: &str = "Apple Contacts file does not exist.";
pub const MESSAGES_DATABASE_MISSING: &str = "Messages database does not exist.";
pub const NOT_AN_IPHONE_BACKUP: &str =
    "This folder is not an iPhone backup, or Messages is missing from it.";
```

Change Display:

```rust
RuntimeError::InvalidOptions(why) => write!(fmt, "{why}"),
```

In `backup.rs`, import those constants. Use `IOS_BACKUP_PASSWORD_INCORRECT` in the existing wrong-password branch.

1. Delete `prompt_for_password`, `IsTerminal`, and `stdin` uses.
2. Add `ios_backup_encrypted_flag` and `password_for_encrypted_backup`:

```rust
/// Whether `backup_root/Manifest.plist` is marked encrypted.
///
/// Returns `None` when the file is missing or cannot be parsed. That is
/// intentional: Import then leaves the password optional and the converter
/// still fails after start if the backup turns out to be encrypted.
pub fn ios_backup_encrypted_flag(backup_root: &Path) -> Option<bool> {
    let path = backup_root.join("Manifest.plist");
    let file = std::fs::File::open(path).ok()?;
    let value = plist::Value::from_reader(file).ok()?;
    let dict = value.as_dictionary()?;
    match dict.get("IsEncrypted") {
        Some(plist::Value::Boolean(flag)) => Some(*flag),
        Some(_) => None,
        None => Some(false),
    }
}

fn password_for_encrypted_backup(provided: Option<&str>) -> Result<String, RuntimeError> {
    match provided {
        Some(password) => Ok(password.to_string()),
        None => Err(RuntimeError::InvalidOptions(
            ENCRYPTED_BACKUP_PASSWORD_REQUIRED.to_string(),
        )),
    }
}

fn reject_leftover_password(
    is_encrypted: bool,
    provided: Option<&str>,
) -> Result<(), RuntimeError> {
    if !is_encrypted && provided.is_some() {
        return Err(RuntimeError::InvalidOptions(
            UNENCRYPTED_BACKUP_CLEAR_PASSWORD.to_string(),
        ));
    }
    Ok(())
}
```

3. In `decrypt_backup`:
   - Unencrypted → `reject_leftover_password(false, options.cleartext_password.as_deref())?` then `Ok(None)`. Drop the `--cleartext-password was provided…` format string.
   - Encrypted + no password → `password_for_encrypted_backup(options.cleartext_password.as_deref())?`
   - Wrong password → keep `IOS_BACKUP_PASSWORD_INCORRECT` (same sentence as today)

Re-export from `lib.rs`:

```rust
pub use backup::ios_backup_encrypted_flag;
pub use error::ENCRYPTED_BACKUP_PASSWORD_REQUIRED;
```

(`error` is private; `pub use` of selected items is allowed.)

Change `--backup-password` help in `cli.rs` to:

```rust
/// iOS backup password (required for encrypted backups; the tool does not prompt)
```

- [ ] **Step 4: Write failing tests for missing paths and not-an-iPhone-backup**

In `crates/exporters/imessage-ir-exporter/src/run.rs` tests, add a helper that builds `ExporterConfig` the same way `src/main.rs` `build_config_from_cli` does (inputs, `SourceConfig::Apple`, `parse_date_range(None, None).unwrap()`, `OutputFormat::Jsonl`, `MediaConfig::default()`). Then:

```rust
#[test]
fn missing_chat_db_uses_locked_copy() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("chat.db");
    let err = options_from_export_config(&apple_cfg(
        &missing,
        AppleConfig {
            platform: Some(ApplePlatform::MacOs),
            ..AppleConfig::default()
        },
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), MESSAGES_DATABASE_MISSING);
}

#[test]
fn missing_attachment_folder_uses_locked_copy() {
    let dir = tempfile::tempdir().unwrap();
    let chat = dir.path().join("chat.db");
    fs::write(&chat, b"sqlite").unwrap();
    let err = options_from_export_config(&apple_cfg(
        &chat,
        AppleConfig {
            platform: Some(ApplePlatform::MacOs),
            attachment_root: Some(dir.path().join("no-such-root").display().to_string()),
            ..AppleConfig::default()
        },
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), ATTACHMENT_FOLDER_MISSING);
}

#[test]
fn missing_apple_contacts_uses_locked_copy() {
    let dir = tempfile::tempdir().unwrap();
    let chat = dir.path().join("chat.db");
    fs::write(&chat, b"sqlite").unwrap();
    let err = options_from_export_config(&apple_cfg(
        &chat,
        AppleConfig {
            platform: Some(ApplePlatform::MacOs),
            apple_contacts: Some(dir.path().join("no-such.abcddb")),
            ..AppleConfig::default()
        },
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), APPLE_CONTACTS_MISSING);
}

#[test]
fn empty_folder_is_not_an_iphone_backup() {
    let dir = tempfile::tempdir().unwrap();
    let err = options_from_export_config(&apple_cfg(
        dir.path(),
        AppleConfig {
            platform: Some(ApplePlatform::Ios),
            ..AppleConfig::default()
        },
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), NOT_AN_IPHONE_BACKUP);
}

#[test]
fn unencrypted_backup_missing_messages_uses_locked_copy() {
    let dir = tempfile::tempdir().unwrap();
    // Manifest.plist present, IsEncrypted false, hashed sms.db missing.
    fs::write(
        dir.path().join("Manifest.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>IsEncrypted</key><false/></dict></plist>"#,
    )
    .unwrap();
    let err = options_from_export_config(&apple_cfg(
        dir.path(),
        AppleConfig {
            platform: Some(ApplePlatform::Ios),
            ..AppleConfig::default()
        },
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), NOT_AN_IPHONE_BACKUP);
}
```

Import the constants from `crate::error`. Do not assert the old `Supplied attachment-root` / `Supplied contacts path` strings.

- [ ] **Step 5: Run those tests and confirm they fail**

Run: `cargo test -p imessage-ir-exporter --lib missing_chat_db missing_attachment missing_apple_contacts empty_folder unencrypted_backup_missing`

Expected: FAIL (still the old CLI sentences, or SQLite/crabapple engine text).

- [ ] **Step 6: Implement locked path checks in `options_from_export_config`**

After `platform` and `db_path` are known:

1. Attachment root: if `Some(path)` and `!Path::new(path).exists()` → `ATTACHMENT_FOLDER_MISSING`. Drop `Supplied attachment-root \`{path}\` does not exist!`.
2. Apple Contacts: if `Some(path)` and `!path.exists()` → `APPLE_CONTACTS_MISSING`. Drop `Supplied contacts path … does not exist!`.
3. `Platform::macOS`: if `!db_path.is_file()` → `MESSAGES_DATABASE_MISSING`.
4. `Platform::iOS`:
   - If `!db_path.is_dir()` or `Manifest.plist` is not a file → `NOT_AN_IPHONE_BACKUP`.
   - If `ios_backup_encrypted_flag(db_path) == Some(false)` and the hashed Messages file (`db_path.join(DEFAULT_PATH_IOS)` from `imessage_database::tables::table`) is not a file → `NOT_AN_IPHONE_BACKUP`.
   - If the flag is `None`, do **not** invent a catalog sentence here; let crabapple fail as engine text.
5. If `get_decrypted_message_database` later hits crabapple `FileNotFoundInBackup` for Messages, map that to `NOT_AN_IPHONE_BACKUP` as well. Leave other `BackupError` variants as `RuntimeError::BackupError`.

Keep the iOS-platform “attachment-root / contacts have no effect” **log** lines. Those are not failures.

- [ ] **Step 7: Write the failing iMessage ffmpeg test**

In `crates/core/message-vault-io-core/src/exporters.rs` tests:

```rust
struct RestoreToolsDir;

impl Drop for RestoreToolsDir {
    fn drop(&mut self) {
        media::set_tools_dir(None);
    }
}

#[test]
fn imessage_convert_without_ffmpeg_uses_locked_copy() {
    let dir = tempfile::tempdir().unwrap();
    let _restore = RestoreToolsDir;
    media::set_tools_dir(Some(dir.path()));
    let form = Form {
        output: "out".into(),
        attachment_media: AttachmentMedia::Convert,
        ..Form::default()
    };
    let err = form.to_config(Exporter::Imessage).unwrap_err();
    assert!(
        err.iter().any(|e| e == CONVERT_COMPRESS_FFMPEG_REQUIRED),
        "{err:?}"
    );
}
```

`CONVERT_COMPRESS_FFMPEG_REQUIRED` must be the spec sentence:

`Convert and Compress need ffmpeg and ffprobe. Put them on PATH, or in the desktop app set the ffmpeg directory in Settings → System.`

Do **not** change the WhatsApp/SMS string in `validate_media`.

- [ ] **Step 8: Run it and confirm it fails, then implement**

Run: `cargo test -p message-vault-io-core imessage_convert_without_ffmpeg`

Expected: FAIL (old `Convert/Compress require ffmpeg…` sentence, or no constant).

Add:

```rust
/// iMessage Import / CLI copy when Convert or Compress is selected and ffmpeg is missing.
pub const CONVERT_COMPRESS_FFMPEG_REQUIRED: &str =
    "Convert and Compress need ffmpeg and ffprobe. Put them on PATH, or in the desktop app set the ffmpeg directory in Settings → System.";
```

Use it in `to_imessage_config` only. The test above can use `super::*`. Do not change `validate_media`.

- [ ] **Step 9: Run exporter + core tests and regenerate CLI docs**

```bash
cargo test -p imessage-ir-exporter --lib
cargo test -p message-vault-io-core imessage_convert_without_ffmpeg
cargo run -p dump-cli-docs -- --output-dir docs/src/content/docs/vault/developer/reference
cargo test -p dump-cli-docs committed_cli_pages_match_dump
```

Expected: all PASS; dump matches the new `--backup-password` help line.

- [ ] **Step 10: Commit**

```bash
git add crates/exporters/imessage-ir-exporter crates/core/message-vault-io-core Cargo.lock \
        docs/src/content/docs/vault/developer/reference/cli/imessage-ir-exporter.md
git commit -m "$(cat <<'EOF'
fix(imessage): use Import-language extract errors

Never prompt for a backup password on stdin. CLI and the desktop
summary now share the locked catalog sentences, without an
Invalid options prefix.
EOF
)"
```

---

### Task 5: Tauri path_stat and encrypted-flag commands

**Files:**
- Modify: `src-tauri/src/commands/paths.rs`
- Modify: `src-tauri/src/main.rs` (register commands)
- Modify: `web/src/lib/tauri.ts`
- Create: `web/src/lib/tauriPaths.ts` only if wrappers would clutter `tauri.ts`; otherwise keep wrappers in `tauri.ts`

**Interfaces:**
- Consumes: `imessage_ir_exporter::ios_backup_encrypted_flag`
- Produces:
  - `path_stat(path: String) -> PathStat { exists, is_file, is_directory }`
  - `ios_backup_encrypted(path: String) -> Option<bool>`
  - TypeScript: `invokePathStat(path: string): Promise<PathStat>` and `invokeIosBackupEncrypted(path: string): Promise<boolean | null>`

Empty / whitespace path: `{ exists: false, is_file: false, is_directory: false }` and encrypted `null`. Do not canonicalize (the path may be a typed string that does not exist yet).

Tauri 2 app commands used from the main window do not need a new capability entry (same as `home_dir`).

- [ ] **Step 1: Write Rust tests in `paths.rs`**

```rust
#[test]
fn path_stat_missing() {
    let stat = path_stat_inner("/no/such/message-vault-path-stat").unwrap();
    assert!(!stat.exists);
    assert!(!stat.is_file);
    assert!(!stat.is_directory);
}

#[test]
fn path_stat_file_and_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("chat.db");
    fs::write(&file, b"sqlite").unwrap();
    let file_stat = path_stat_inner(file.to_str().unwrap()).unwrap();
    assert!(file_stat.exists && file_stat.is_file && !file_stat.is_directory);
    let dir_stat = path_stat_inner(dir.path().to_str().unwrap()).unwrap();
    assert!(dir_stat.exists && dir_stat.is_directory && !dir_stat.is_file);
}

#[test]
fn blank_path_is_missing() {
    let stat = path_stat_inner("  ").unwrap();
    assert!(!stat.exists);
}
```

Keep `path_stat_inner` as a plain function the `#[tauri::command]` wraps, so tests do not need a Tauri runtime.

- [ ] **Step 2: Run tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml path_stat`

Expected: FAIL.

- [ ] **Step 3: Implement commands**

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStat {
    pub exists: bool,
    pub is_file: bool,
    pub is_directory: bool,
}

pub(crate) fn path_stat_inner(path: &str) -> Result<PathStat, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(PathStat {
            exists: false,
            is_file: false,
            is_directory: false,
        });
    }
    let path = Path::new(trimmed);
    Ok(PathStat {
        exists: path.exists(),
        is_file: path.is_file(),
        is_directory: path.is_dir(),
    })
}

#[tauri::command]
pub fn path_stat(path: String) -> Result<PathStat, String> {
    path_stat_inner(&path)
}

#[tauri::command]
pub fn ios_backup_encrypted(path: String) -> Result<Option<bool>, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(imessage_ir_exporter::ios_backup_encrypted_flag(Path::new(trimmed)))
}
```

Register both in `src-tauri/src/main.rs` `invoke_handler`.

In `web/src/lib/tauri.ts`:

```typescript
export interface PathStat {
  exists: boolean;
  isFile: boolean;
  isDirectory: boolean;
}

export async function invokePathStat(path: string): Promise<PathStat> {
  return invoke("path_stat", { path });
}

export async function invokeIosBackupEncrypted(path: string): Promise<boolean | null> {
  return invoke("ios_backup_encrypted", { path });
}
```

Serde `camelCase` on `PathStat` matches `isFile` / `isDirectory`.

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml path_stat
cargo test -p imessage-ir-exporter --lib
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/paths.rs src-tauri/src/main.rs web/src/lib/tauri.ts
git commit -m "$(cat <<'EOF'
feat(tauri): probe import paths and iOS backup encryption

The Import form cannot see the filesystem from the WebView. Return
whether a path exists and whether Manifest.plist is marked encrypted.
EOF
)"
```

---

### Task 6: Pass attachment root, Apple Contacts, and jailbreak through extract

**Files:**
- Modify: `src-tauri/src/commands/extract.rs`
- Modify: `web/src/lib/types.ts` (`ExtractConfig`)
- Modify: `web/src/lib/tauri.ts` (`invokeExtract` args)
- Create: `web/src/lib/imessageExtractFields.ts`
- Create: `web/src/lib/imessageExtractFields.test.ts`

**Interfaces:**
- Consumes: `Form.attachment_root`, `Form.apple_contacts`, `ApplePlatform::MacOs` / `Ios` already on `Form::to_config`
- Produces:
  - `ExtractArgs.attachment_root: Option<String>`
  - `ExtractArgs.apple_contacts: Option<String>`
  - `build_exporter_config` match arm `"imessage-ios" | "imessage-macos" | "imessage-jailbreak"`
  - jailbreak → `ApplePlatform::MacOs`
  - `imessageExtractFields(...)` returning the extract fields to spread into `invokeExtract`

Today `useImportJob` only sends `attachment_media` when `form.isIos`. After this task the helper sends media for all three methods; password only for `imessage-ios`; extras only for Mac/jailbreak when non-empty.

- [ ] **Step 1: Write the TypeScript helper tests**

`web/src/lib/imessageExtractFields.ts` tests:

```typescript
import { describe, expect, it } from "vitest";
import { imessageExtractFields } from "./imessageExtractFields";

describe("imessageExtractFields", () => {
  it("sends media and password for iPhone backup, not attachment root", () => {
    expect(
      imessageExtractFields({
        source: "imessage-ios",
        backupPassword: "pw",
        attachmentMedia: "convert",
        maxResolution: "1080p",
        maxFps: "30",
        minSizeMb: "20",
        obfuscate: true,
        attachmentRoot: "/should-not-send",
        appleContacts: "/should-not-send",
      }),
    ).toEqual({
      attachment_media: "convert",
      media_max_resolution: "1080p",
      media_max_fps: "30",
      media_min_size: "20M",
      obfuscate: true,
      backup_password: "pw",
    });
  });

  it("omits empty extras on Mac and does not send a password", () => {
    expect(
      imessageExtractFields({
        source: "imessage-macos",
        backupPassword: "leftover",
        attachmentMedia: "copy",
        maxResolution: "720p",
        maxFps: "30",
        minSizeMb: "20",
        obfuscate: false,
        attachmentRoot: "  ",
        appleContacts: "",
      }),
    ).toEqual({
      attachment_media: "copy",
      media_max_resolution: "720p",
      media_max_fps: "30",
      media_min_size: "20M",
    });
  });

  it("sends attachment root and contacts for jailbreak", () => {
    expect(
      imessageExtractFields({
        source: "imessage-jailbreak",
        backupPassword: "",
        attachmentMedia: "skip",
        maxResolution: "720p",
        maxFps: "30",
        minSizeMb: "20",
        obfuscate: false,
        attachmentRoot: "/mnt/iphone/Library/SMS",
        appleContacts: "/mnt/iphone/AddressBook.sqlitedb",
      }),
    ).toEqual({
      attachment_media: "skip",
      media_max_resolution: "720p",
      media_max_fps: "30",
      media_min_size: "20M",
      attachment_root: "/mnt/iphone/Library/SMS",
      apple_contacts: "/mnt/iphone/AddressBook.sqlitedb",
    });
  });
});
```

- [ ] **Step 2: Run and confirm fail**

Run: `cd web && npx vitest run src/lib/imessageExtractFields.test.ts`

Expected: FAIL.

- [ ] **Step 3: Implement `imessageExtractFields`**

Reuse `mediaExtractFields` from `sbrExtractFields.ts`. Include `backup_password` only for `imessage-ios` when trimmed non-empty (still pass through even if empty? Spec: omit empty). Include `obfuscate` only for `imessage-ios`. Include `attachment_root` / `apple_contacts` only for `imessage-macos` and `imessage-jailbreak` when trimmed non-empty.

- [ ] **Step 4: Write Rust extract config tests**

In `src-tauri/src/commands/extract.rs` tests, extend `ExtractOptions` and `test_options` with:

```rust
attachment_root: String,
apple_contacts: String,
```

Default both to empty in `test_options`. Add:

```rust
#[test]
fn jailbreak_uses_macos_platform_and_attachment_root() {
    let mut options = test_options(Vec::new());
    options.attachment_root = "/mnt/iphone/Library/SMS".into();
    options.apple_contacts = "/mnt/iphone/AddressBook.sqlitedb".into();
    let config = build_exporter_config(
        "imessage-jailbreak",
        "/mnt/iphone/sms.db",
        "/tmp/out",
        &options,
    )
    .unwrap();
    match config.source {
        SourceConfig::Apple(apple) => {
            assert_eq!(apple.platform, Some(ApplePlatform::MacOs));
            assert_eq!(
                apple.attachment_root.as_deref(),
                Some("/mnt/iphone/Library/SMS")
            );
            assert_eq!(
                apple.apple_contacts.as_deref(),
                Some(std::path::Path::new("/mnt/iphone/AddressBook.sqlitedb"))
            );
            assert!(apple.backup_password.is_none());
        }
        other => panic!("expected Apple, got {other:?}"),
    }
}

#[test]
fn ios_backup_does_not_forward_attachment_root() {
    let mut options = test_options(Vec::new());
    options.attachment_root = "/ignored".into();
    options.backup_password = "pw".into();
    let config =
        build_exporter_config("imessage-ios", "/backups/iphone", "/tmp/out", &options).unwrap();
    match config.source {
        SourceConfig::Apple(apple) => {
            assert_eq!(apple.platform, Some(ApplePlatform::Ios));
            assert_eq!(apple.backup_password.as_deref(), Some("pw"));
            // Form still copies the string if set; the UI must omit it.
            // This test documents extract.rs: only fill Form.attachment_root
            // when the source is macos or jailbreak.
            assert!(apple.attachment_root.is_none());
        }
        other => panic!("expected Apple, got {other:?}"),
    }
}

#[test]
fn macos_forwards_optional_attachment_root() {
    let mut options = test_options(Vec::new());
    options.attachment_root = "/Users/sam/Library/Messages".into();
    let config = build_exporter_config(
        "imessage-macos",
        "/Users/sam/Library/Messages/chat.db",
        "/tmp/out",
        &options,
    )
    .unwrap();
    match config.source {
        SourceConfig::Apple(apple) => {
            assert_eq!(apple.platform, Some(ApplePlatform::MacOs));
            assert_eq!(
                apple.attachment_root.as_deref(),
                Some("/Users/sam/Library/Messages")
            );
        }
        other => panic!("expected Apple, got {other:?}"),
    }
}
```

For the iOS test: `build_exporter_config` must **not** copy `options.attachment_root` onto `Form` when `source == "imessage-ios"`. Same for `apple_contacts`. Password is copied only for `imessage-ios`.

- [ ] **Step 5: Run Rust tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml jailbreak_uses_macos ios_backup_does_not macos_forwards`

Expected: FAIL (`imessage-jailbreak` hits `unsupported source`).

- [ ] **Step 6: Implement extract.rs and invokeExtract**

Add to `ExtractArgs` (camelCase serde already):

```rust
pub attachment_root: Option<String>,
pub apple_contacts: Option<String>,
```

Add the same fields to `ExtractOptions`. Parse with `optional_trimmed` into owned `String` (empty if absent).

Change the match arm to `"imessage-ios" | "imessage-macos" | "imessage-jailbreak"`. Set:

```rust
apple_platform: if source == "imessage-ios" {
    ApplePlatform::Ios
} else {
    ApplePlatform::MacOs
},
attachment_root: if source == "imessage-ios" {
    String::new()
} else {
    options.attachment_root.clone()
},
apple_contacts: if source == "imessage-ios" {
    String::new()
} else {
    options.apple_contacts.clone()
},
backup_password: if source == "imessage-ios" {
    options.backup_password.clone()
} else {
    String::new()
},
```

`Form` already omits empty `attachment_root` / `apple_contacts` via `non_empty`.

`web/src/lib/types.ts` `ExtractConfig`:

```typescript
attachment_root?: string;
apple_contacts?: string;
```

`invokeExtract` args: `attachmentRoot: config.attachment_root ?? null`, `appleContacts: config.apple_contacts ?? null`.

- [ ] **Step 7: Run tests**

```bash
cd web && npx vitest run src/lib/imessageExtractFields.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/extract.rs web/src/lib/types.ts web/src/lib/tauri.ts \
        web/src/lib/imessageExtractFields.ts web/src/lib/imessageExtractFields.test.ts
git commit -m "$(cat <<'EOF'
feat(import): pass iMessage attachment root and contacts

Jailbreak copies use the Mac database layout. The extract command now
forwards the attachment folder and AddressBook file those runs need.
EOF
)"
```

---

### Task 7: Wire ImportScreen, remembered extras, probes, and the job

**Files:**
- Modify: `web/src/screens/ImportScreen.tsx`
- Modify: `web/src/screens/import/useImportJob.ts`
- Create: `web/src/screens/import/imessagePathProbe.ts` (optional small helper) + test if the probe logic is more than a `useEffect`

**Interfaces:**
- Consumes: Task 1–6 helpers and Tauri commands
- Produces: a working form that probes paths, pre-fills Mac `chat.db` on macOS only, remembers extras per method, and starts extract with `imessageExtractFields`

Behavior:

1. `source` state default `IMESSAGE_DEFAULT_METHOD` (`imessage-ios`).
2. Keep `lastImessageMethod` so choosing **iMessage** again after WhatsApp restores the last method instead of always resetting. First visit is still iPhone backup.
3. On `source` change, load `getImporterPath(source)` and, if `isImessageMethod(source)`, `getImporterExtraPaths(source)`.
4. When switching **to** `imessage-macos` and the loaded backup path is empty, call `invokeHomeDir()`. If `os === "macos"`, `invokePathStat(macMessagesDbPath(home))`. If the file exists, set backup path via `shouldPrefillMacMessagesDb`.
5. When `backupPath`, `attachmentRoot`, or `appleContacts` change, `invokePathStat` each non-empty path. For `imessage-ios` with an existing directory, also `invokeIosBackupEncrypted(backupPath)`. Debounce 200ms to avoid a command per keystroke.
6. Persist extras with `setImporterExtraPath` when remember-paths is on (same pattern as `updateBackupPath`).
7. `startImport` uses `imessageExtractFields` when `isImessageMethod(source)` instead of the `isIos` ternary. Keep the SBR branch. Drop `isIos` from `ImportJobFormValues`; pass `attachmentRoot` and `appleContacts`.
8. Switching methods must not copy Mac `chat.db` into the iPhone folder field: because paths are stored per method id, loading on source change is enough. Do not assign `backupPath` across methods except through `getImporterPath(newSource)`.

- [ ] **Step 1: Write a probe-helper test if the debounce/merge lives in a pure function**

Prefer a pure `mergeImessageStats` so the effect stays thin:

```typescript
export function emptyImessagePathStats(): ImessagePathStats {
  return {
    backup: null,
    attachmentRoot: null,
    appleContacts: null,
    backupEncrypted: null,
  };
}
```

When the method is not `imessage-ios`, force `backupEncrypted: null` even if a previous iOS probe returned `true`.

Test that in `web/src/lib/imessageImport.test.ts`:

```typescript
it("clears encryption state when leaving iPhone backup", () => {
  expect(
    imessageStatsForMethod("imessage-macos", {
      backup: presentFile,
      attachmentRoot: null,
      appleContacts: null,
      backupEncrypted: true,
    }),
  ).toEqual({
    backup: presentFile,
    attachmentRoot: null,
    appleContacts: null,
    backupEncrypted: null,
  });
});
```

Add `imessageStatsForMethod(method, stats)` in `imessageImport.ts`.

- [ ] **Step 2: Run and fail, then implement the helper**

Run: `cd web && npx vitest run src/lib/imessageImport.test.ts`

- [ ] **Step 3: Wire `useImportJob`**

Replace the `isIos` spread with:

```typescript
...(isImessageMethod(form.source)
  ? imessageExtractFields({
      source: form.source,
      backupPassword: form.backupPassword,
      attachmentMedia: form.attachmentMedia,
      maxResolution: form.maxResolution,
      maxFps: form.maxFps,
      minSizeMb: form.minSizeMb,
      obfuscate: form.obfuscate,
      attachmentRoot: form.attachmentRoot,
      appleContacts: form.appleContacts,
    })
  : {}),
```

Extend `ImportJobFormValues` with `attachmentRoot: string` and `appleContacts: string`. Remove `isIos`.

ImportScreen `onImport` must pass those strings.

- [ ] **Step 4: Wire `ImportScreen`**

State:

```typescript
const [source, setSource] = useState(DEFAULT_SOURCE); // still "imessage-ios"
const [attachmentRoot, setAttachmentRoot] = useState("");
const [appleContacts, setAppleContacts] = useState("");
const [pathStats, setPathStats] = useState(emptyImessagePathStats);
```

`onSourceChange`:

```typescript
function handleSourceChange(next: string): void {
  const resolved =
    next === IMESSAGE_SOURCE_ID ? lastImessageMethodRef.current : next;
  setSource(resolved);
  if (isImessageMethod(resolved)) lastImessageMethodRef.current = resolved;
}
```

When `resolved` is an iMessage method, load remembered backup + extras. Then, if `resolved === "imessage-macos"`, run Mac pre-fill (async). Reset `pathStats` to `emptyImessagePathStats()` so Import stays disabled until the probe returns.

Probe `useEffect` dependencies: `source`, `backupPath`, `attachmentRoot`, `appleContacts`. Skip Tauri invokes when `!isTauri()`. Map invoke results into `PathStat` (`isFile` from camelCase).

Never pre-fill iPhone backup or jailbreak paths from the home directory.

- [ ] **Step 5: Run frontend tests and typecheck**

```bash
cd web && npx vitest run src/lib/imessageImport.test.ts src/lib/imessageExtractFields.test.ts src/screens/import/ImportFormFields.test.tsx src/lib/exportSources.test.ts src/lib/system-settings.test.ts
cd web && npx tsc --noEmit
cd web && npx biome check src/screens/ImportScreen.tsx src/screens/import
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/screens/ImportScreen.tsx web/src/screens/import/useImportJob.ts web/src/lib/imessageImport.ts web/src/lib/imessageImport.test.ts
git commit -m "$(cat <<'EOF'
feat(import): probe iMessage paths and start extract per method

Remember folders per method, pre-fill Mac chat.db only on macOS, and
require a password once Manifest.plist says the backup is encrypted.
EOF
)"
```

---

### Task 8: User docs, changelog, and full verify

**Files:**
- Modify: `docs/src/content/docs/vault/user/import-from-a-backup.md`
- Modify: `docs/src/content/docs/vault/user/prepare-a-backup/iphone-ipad.md`
- Modify: `docs/src/content/docs/vault/user/how-to/troubleshooting.md`
- Modify: `CHANGELOG.md` under `[Unreleased]`

**Interfaces:**
- Consumes: finished UI copy (iMessage + three methods)
- Produces: docs that no longer say **iPhone - iOS** / **iMessage - macOS** as two sources

- [ ] **Step 1: Update Import from a backup**

Replace the source table rows for Apple with:

```markdown
   | **iMessage** → **iPhone backup** | Finder/iTunes backup folder (device UUID directory), not `sms.db` inside it |
   | **iMessage** → **Mac Messages** | `chat.db` (optional attachment folder if `Attachments` is not next to the database) |
   | **iMessage** → **Jailbroken iPhone** | `sms.db` plus the folder that contains `Attachments` and `StickerCache` |
```

Keep WhatsApp and SMS Backup & Restore rows. Mention that iPhone backup shows Encryption password, required when the backup is encrypted.

- [ ] **Step 2: Update iPhone or iPad prepare guide**

Add a third “How to get the data” subsection for a jailbreak filesystem copy: point at `sms.db`, then at the Messages root that contains `Attachments` and `StickerCache`. Do not tell people to pick that tree as an iPhone backup folder.

Change the “Next step” sentence to: choose **iMessage**, then the method that matches the files.

- [ ] **Step 3: Update troubleshooting**

Replace “set **iPhone - iOS** or **iMessage - macOS**” with: choose **iMessage** and the matching method (iPhone backup vs Mac Messages vs Jailbroken iPhone). A `.db` file is not an iPhone backup folder. If Import says the backup is not encrypted, clear **Encryption password**. If it says the backup is encrypted, fill that field. Convert/Compress without ffmpeg: put the tools on PATH, or in the desktop app set the ffmpeg directory in Settings → System.

- [ ] **Step 4: Changelog**

Under `[Unreleased]` **Changed** (date `2026-08-26`):

```markdown
- 2026-08-26: Import lists one **iMessage** source with methods Mac Messages, iPhone backup, and Jailbroken iPhone. Mac and jailbreak can set an attachment folder and an Apple Contacts file. Encrypted iPhone backups require the password in the form; the app does not prompt in a terminal. Extract errors for missing paths, leftover password, and missing ffmpeg use the locked Import-language sentences.
```

Under **Added** if a separate bullet is clearer for jailbreak; do not duplicate. One Changed bullet is enough.

- [ ] **Step 5: Full verify**

```bash
cd web && npx vitest run && npx biome ci .
cargo fmt --all -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test -p imessage-ir-exporter
cargo test --manifest-path src-tauri/Cargo.toml
cargo test -p dump-cli-docs committed_cli_pages_match_dump
```

Expected: all pass. If `biome ci` fails, run `cd web && npx biome check --write` on the touched files and fix for real (no `biome-ignore`).

Import cannot be verified in Playwright against Vite. If a desktop window is already running (`cargo tauri dev`), spot-check: source list shows **iMessage** once; method dropdown; jailbreak Import disabled without attachment folder. Do not start a second Vite server.

- [ ] **Step 6: Commit**

```bash
git add docs/src/content/docs/vault/user/import-from-a-backup.md \
        docs/src/content/docs/vault/user/prepare-a-backup/iphone-ipad.md \
        docs/src/content/docs/vault/user/how-to/troubleshooting.md \
        CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs: describe the unified iMessage import methods

The desktop Import screen no longer lists separate iPhone and Mac
sources. The user guide now matches the three extraction methods.
EOF
)"
```

---

## Spec coverage (self-review)

| Spec requirement | Task |
|---|---|
| One iMessage row, three named methods | 3, 7 |
| Default method iPhone backup | 1, 7 |
| Ids and staging slugs including `iphone-jailbreak` | 1, 2, 6 |
| Remembered paths per method; extras per method; no cross-paste | 2, 7 |
| Field table (path kinds, password, attachment root, contacts, attachments, vault contacts) | 1, 3 |
| Hide `--platform`; derive macOS/iOS | 1, 6 |
| Empty optional extras omitted | 6 |
| Jailbreak = Mac layout + required attachment root | 1, 6 |
| Path must exist; optional extras existence | 1, 5, 7 |
| Path kind errors (.db vs folder) | 1, 3 |
| Encrypted: password required when Manifest says so; unknown → optional; fail after start with locked copy | 1, 4, 5, 7 |
| Never prompt on a terminal | 4 |
| Wrong password `The iOS backup password was incorrect.` | 4 |
| Leftover password `This backup is not encrypted. Clear Encryption password.` | 4 |
| No `Invalid options!` prefix on catalog sentences | 4 |
| Attachment folder is a file / Apple Contacts is a directory (form) | 1, 3 |
| After-start: missing attachment folder / Apple Contacts / messages db / not an iPhone backup | 4 |
| Convert/Compress missing ffmpeg (iMessage Import-language sentence) | 4 |
| Engine text stays for disk, SQLite permissions, other crabapple parse, cancel, all-media-failed | 4 (do not rewrite) |
| Mac pre-fill `~/Library/Messages/chat.db` on macOS only | 1, 7 |
| No pre-fill for iPhone/jailbreak/Linux/Windows | 1, 7 |
| `--use-message-times` not in UI and not passed | 3, 6 (no new field) |
| WhatsApp / SBR unchanged | 3 |
| User docs | 8 |
| Apple Contacts empty → auto-scan warning is converter-side, continue | 4 (no new GUI error for empty contacts) |
| Form and extract errors match the locked catalog | 1, 3, 4 |

No placeholders remain in the task steps above. Method ids, `ExtractArgs` field names, and error strings are the same in later tasks as in Task 1 and Task 4.
