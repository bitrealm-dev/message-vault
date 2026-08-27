# WhatsApp Import Methods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Import screen’s two WhatsApp sources with one **WhatsApp** source that has Platform Android or iPhone, and pass `wtsexporter` the folder, crypt backup, key, contacts database, media folder, message-database override, and Business flag each platform actually uses.

**Architecture:** Keep extract/staging ids `whatsapp-android` and `whatsapp-ios`. The source dropdown shows a single **WhatsApp** row; a second dropdown labeled **Platform** picks Android or iPhone. Pure TypeScript decides which fields show, when the Android key is required (crypt file in the folder, no `msgstore.db`), and when Import is enabled. The converter looks up `msgstore.db.crypt12` / `.crypt14` / `.crypt15` in the folder root when Android has no decrypted `msgstore.db`. Tauri today sets only the platform on `WhatsappConfig` and leaves key/`-b`/`-w`/`-m`/`-d`/`--business` empty.

**Tech Stack:** React 19 + TypeScript in `web/`, Vitest + Testing Library, Tauri v2 commands in `src-tauri/`, `whatsapp-exporter` + `message-vault-io-core` `WhatsappConfig`.

**Spec:** `docs/superpowers/specs/2026-08-26-whatsapp-import-methods-design.md`

## Global Constraints

- Do not change iMessage, SMS Backup & Restore, or other non-WhatsApp Import sources except the source list (it loses the two WhatsApp rows).
- Do not show `-a` / `-i` as controls. Derive them: Android → `-a`; iPhone → `-i`.
- Do not add `-e`, `--call-db`, `--wab`, HTML/JSON extras, merge, date filters, vCard enrich, or `--move-media`.
- Do not put an Apple backup-password field on the WhatsApp form. Do not reuse `backup_password` for the Android key.
- Never persist the decryption key. Never prompt for it on stdin.
- Prefer `msgstore.db` over a crypt file in the same folder. Pass `-k` only when a crypt file is forwarded as `-b`.
- Look for crypt files in the backup folder **root** only. Names: `msgstore.db.crypt12`, `msgstore.db.crypt14`, `msgstore.db.crypt15`.
- User-facing form errors are the locked catalog in the spec. Copy those sentences verbatim.
- Required path empty: no extra sentence. The label already has a red star.
- Default Platform on first open: Android (`whatsapp-android`).
- Existing `whatsapp-android` / `whatsapp-ios` remembered backup paths must keep working.
- Empty optional `-w` / `-m` / `-d` are omitted so `wtsexporter` defaults still run.
- `--business` is iPhone only, off by default, not remembered.
- Attachments Copy / Convert / Compress / Skip and vault Contacts apply to both WhatsApp platforms and must be passed on extract (today they are not).
- Obfuscate stays off the WhatsApp form.
- Import is desktop-only (`isTauri()`). Prove behavior with Vitest and Rust tests, not Playwright against Vite.
- Prefer a real fix over `biome-ignore`. Prefix unused bindings with `_`.
- Never commit to `main`. Work on `feat/whatsapp-import-methods`.
- Product version files stay at the current lockstep value. Do not bump versions.
- Follow `web/` Biome + existing Import form styling (`StackedField`, `Select`, `PathPicker`, red `*` / `(Optional)`). Do not invent a new visual language.

## File map

| File | Responsibility |
|---|---|
| `web/src/lib/whatsappImport.ts` | Platform ids, labels, which fields show, crypt-required rule, Import-enable rules, path-kind errors |
| `web/src/lib/whatsappExtractFields.ts` | Build the extract payload (media always; key/wa/media/db only when non-empty; business only on iPhone when true) |
| `web/src/lib/exportSources.ts` | One **WhatsApp** row in the source list |
| `web/src/lib/system-settings.ts` | Remember optional WhatsApp path extras per method id (not the key) |
| `web/src/lib/types.ts` / `web/src/lib/tauri.ts` | WhatsApp extract fields on `ExtractConfig` / `invokeExtract` |
| `web/src/screens/import/ImportFormFields.tsx` | Platform dropdown and per-platform fields |
| `web/src/screens/ImportScreen.tsx` | Platform state, remembered extras, live path + crypt probe |
| `web/src/screens/import/useImportJob.ts` | Pass WhatsApp extract fields |
| `src-tauri/src/commands/extract.rs` | Accept WhatsApp extras; fill `WhatsappConfig`; iPhone folder → `backup` |
| `crates/exporters/whatsapp-exporter/src/wtsexporter.rs` | Android crypt-file lookup when `-b` is unset |
| `docs/src/content/docs/vault/user/import-from-a-backup.md` | Source/Platform table |
| `docs/src/content/docs/vault/user/prepare-a-backup/android-whatsapp.md` | Open Import → WhatsApp → Platform Android |
| `docs/src/content/docs/vault/user/prepare-a-backup/iphone-whatsapp.md` | Same for iPhone; drop Apple backup-password instruction |
| `CHANGELOG.md` | Unreleased note dated 2026-08-26 |

Out of scope files: `crates/message-vault-io-gui/**`, `web-next/**`, vault server schema, iMessage form helpers except the shared source list.

---

### Task 0: Branch and record the spec

**Files:**
- Create: this plan at `docs/superpowers/plans/2026-08-26-whatsapp-import-methods.md`
- Create: `docs/superpowers/specs/2026-08-26-whatsapp-import-methods-design.md`

**Interfaces:**
- Consumes: locked spec on disk
- Produces: git branch `feat/whatsapp-import-methods` with spec + plan committed

- [ ] **Step 1: Confirm the branch**

```bash
cd /home/mbeisser/repo/message-vault
git branch --show-current
```

Expected: `feat/whatsapp-import-methods`. Stop if this prints `main`.

- [ ] **Step 2: Commit the spec and this plan** (done in the design session if already committed; skip if `git status` is clean)

```bash
git add docs/superpowers/specs/2026-08-26-whatsapp-import-methods-design.md \
        docs/superpowers/plans/2026-08-26-whatsapp-import-methods.md
git commit -m "$(cat <<'EOF'
docs: add WhatsApp import methods spec and plan

Lock one WhatsApp source, Android and iPhone platforms, field
table, validation, and which wtsexporter flags stay off the form.
EOF
)"
```

---

### Task 1: Platform catalog and Import-enable rules

**Files:**
- Create: `web/src/lib/whatsappImport.ts`
- Create: `web/src/lib/whatsappImport.test.ts`

**Interfaces:**
- Consumes: spec tables for platforms, fields, enable rules, path kind, crypt preference
- Produces:
  - `WHATSAPP_SOURCE_ID = "whatsapp"`
  - `WHATSAPP_DEFAULT_METHOD = "whatsapp-android"`
  - `WHATSAPP_METHODS`: `{ id, label }[]` with ids `whatsapp-android`, `whatsapp-ios` and labels `Android`, `iPhone`
  - `WhatsappMethodId`
  - `isWhatsappMethod(source: string): source is WhatsappMethodId`
  - `whatsappShowsKey(method)`, `whatsappShowsMedia(method)`, `whatsappShowsDb(method)`, `whatsappShowsBusiness(method)` — true only for Android (key/media/db) or iPhone (business)
  - `whatsappShowsContactsDb(method)` — true for both
  - `WHATSAPP_CRYPT_NAMES = ["msgstore.db.crypt12", "msgstore.db.crypt14", "msgstore.db.crypt15"]`
  - `whatsappCryptRequired(hasMsgstoreDb: boolean, cryptName: string | null): boolean` — `!hasMsgstoreDb && cryptName !== null`
  - `WhatsappPathStats` with `backup`, `contactsDb`, `media`, `db`, `hasMsgstoreDb`, `cryptName`
  - `emptyWhatsappPathStats()`
  - Error constants matching the spec catalog
  - `whatsappCanImport({ method, backupPath, key, contactsDb, media, db, stats })` → `{ enabled, errors }`

- [ ] **Step 1: Write the failing tests**

```typescript
import { describe, expect, it } from "vitest";
import {
  WHATSAPP_DEFAULT_METHOD,
  WHATSAPP_ERR_CRYPT_KEY,
  WHATSAPP_ERR_FOLDER_IS_FILE,
  WHATSAPP_ERR_MUST_BE_FILE,
  WHATSAPP_ERR_MUST_BE_FOLDER,
  WHATSAPP_ERR_PATH_MISSING,
  WHATSAPP_SOURCE_ID,
  isWhatsappMethod,
  whatsappCanImport,
  whatsappCryptRequired,
  whatsappShowsBusiness,
  whatsappShowsKey,
} from "./whatsappImport";

const dir = { exists: true, isFile: false, isDirectory: true };
const file = { exists: true, isFile: true, isDirectory: false };

describe("whatsappImport", () => {
  it("keeps method ids and defaults Android", () => {
    expect(WHATSAPP_SOURCE_ID).toBe("whatsapp");
    expect(WHATSAPP_DEFAULT_METHOD).toBe("whatsapp-android");
    expect(isWhatsappMethod("whatsapp-android")).toBe(true);
    expect(isWhatsappMethod("whatsapp-ios")).toBe(true);
    expect(isWhatsappMethod("whatsapp")).toBe(false);
    expect(whatsappShowsKey("whatsapp-android")).toBe(true);
    expect(whatsappShowsKey("whatsapp-ios")).toBe(false);
    expect(whatsappShowsBusiness("whatsapp-ios")).toBe(true);
    expect(whatsappShowsBusiness("whatsapp-android")).toBe(false);
  });

  it("requires a key only when a crypt file is used", () => {
    expect(whatsappCryptRequired(true, "msgstore.db.crypt15")).toBe(false);
    expect(whatsappCryptRequired(false, "msgstore.db.crypt15")).toBe(true);
    expect(whatsappCryptRequired(false, null)).toBe(false);
  });

  it("disables Import when the Android folder is a file", () => {
    const result = whatsappCanImport({
      method: "whatsapp-android",
      backupPath: "/tmp/msgstore.db",
      key: "",
      contactsDb: "",
      media: "",
      db: "",
      stats: {
        backup: file,
        contactsDb: null,
        media: null,
        db: null,
        hasMsgstoreDb: true,
        cryptName: null,
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.backupPath).toBe(WHATSAPP_ERR_FOLDER_IS_FILE);
  });

  it("requires the key when only a crypt file is present", () => {
    const result = whatsappCanImport({
      method: "whatsapp-android",
      backupPath: "/tmp/wa",
      key: "",
      contactsDb: "",
      media: "",
      db: "",
      stats: {
        backup: dir,
        contactsDb: null,
        media: null,
        db: null,
        hasMsgstoreDb: false,
        cryptName: "msgstore.db.crypt15",
      },
    });
    expect(result.enabled).toBe(false);
    expect(result.errors.key).toBe(WHATSAPP_ERR_CRYPT_KEY);
  });

  it("enables Android Import for a folder with msgstore.db and no key", () => {
    const result = whatsappCanImport({
      method: "whatsapp-android",
      backupPath: "/tmp/wa",
      key: "",
      contactsDb: "",
      media: "",
      db: "",
      stats: {
        backup: dir,
        contactsDb: null,
        media: null,
        db: null,
        hasMsgstoreDb: true,
        cryptName: "msgstore.db.crypt15",
      },
    });
    expect(result.enabled).toBe(true);
    expect(result.errors).toEqual({});
  });
});
```

Also assert `WHATSAPP_ERR_PATH_MISSING === "This path does not exist."`, optional contacts path that is a directory → `WHATSAPP_ERR_MUST_BE_FILE`, optional media path that is a file → `WHATSAPP_ERR_MUST_BE_FOLDER`, empty backup path → `{ enabled: false, errors: {} }`, iPhone directory with no key → enabled.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd web && npx vitest run src/lib/whatsappImport.test.ts`

Expected: FAIL (module missing).

- [ ] **Step 3: Implement `whatsappImport.ts`**

Mirror `imessageImport.ts`. Empty required folder: return `{ enabled: false, errors: {} }` with no sentence. Pending stats (`backup === null`) keep Import off with no error. `cryptName` is the first existing crypt filename or `null`.

- [ ] **Step 4: Re-run tests**

Run: `cd web && npx vitest run src/lib/whatsappImport.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/whatsappImport.ts web/src/lib/whatsappImport.test.ts
git commit -m "$(cat <<'EOF'
feat(import): add WhatsApp platform catalog and enable rules

Decide Android vs iPhone fields and when a crypt folder needs a key
before the Import form starts showing those controls.
EOF
)"
```

---

### Task 2: One WhatsApp row in the source list

**Files:**
- Modify: `web/src/lib/exportSources.ts`
- Modify: `web/src/lib/exportSources.test.ts`
- Modify: `web/src/lib/imessageImport.test.ts` (the `isImessageMethod("whatsapp-android")` check stays valid)

**Interfaces:**
- Consumes: `WHATSAPP_SOURCE_ID`
- Produces: `EXPORT_SOURCES` contains `{ id: "whatsapp", label: "WhatsApp" }` and no longer lists `whatsapp-android` / `whatsapp-ios` as top-level rows

- [ ] **Step 1: Update the failing source-list test**

In `exportSources.test.ts`, after the iMessage assertions:

```typescript
expect(ids).toContain(WHATSAPP_SOURCE_ID);
expect(ids).not.toContain("whatsapp-android");
expect(ids).not.toContain("whatsapp-ios");
expect(EXPORT_SOURCES.find((s) => s.id === WHATSAPP_SOURCE_ID)?.label).toBe("WhatsApp");
```

Remove `expect(ids).toContain("whatsapp-android")`.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cd web && npx vitest run src/lib/exportSources.test.ts`

Expected: FAIL (`whatsapp-android` still listed).

- [ ] **Step 3: Replace the two WhatsApp rows with one `WHATSAPP_SOURCE_ID` row** in `exportSources.ts` (keep it next to iMessage).

- [ ] **Step 4: Re-run**

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/exportSources.ts web/src/lib/exportSources.test.ts
git commit -m "$(cat <<'EOF'
feat(import): list one WhatsApp source instead of two

Match the iMessage source-list pattern so Platform can pick Android
or iPhone under a single row.
EOF
)"
```

---

### Task 3: Extract payload helper

**Files:**
- Create: `web/src/lib/whatsappExtractFields.ts`
- Create: `web/src/lib/whatsappExtractFields.test.ts`
- Modify: `web/src/lib/types.ts` (`ExtractConfig`)

**Interfaces:**
- Consumes: `WhatsappMethodId`, `mediaExtractFields` from `sbrExtractFields.ts`
- Produces: `whatsappExtractFields(args)` → pick of `ExtractConfig` with:
  - `attachment_media` / resolution / fps / min size always
  - `whatsapp_key` only when trimmed non-empty (Android)
  - `whatsapp_wa` / `whatsapp_media` / `whatsapp_db` only when trimmed non-empty
  - `whatsapp_business` only when method is `whatsapp-ios` and `business` is true
  - never sets `backup_password`

Add to `ExtractConfig`:

```typescript
whatsapp_key?: string;
whatsapp_wa?: string;
whatsapp_media?: string;
whatsapp_db?: string;
whatsapp_business?: boolean;
```

- [ ] **Step 1: Write tests** that Android with a key and empty wa omits `whatsapp_wa` and `whatsapp_business`; iPhone with business true sets `whatsapp_business: true` and omits `whatsapp_key`.

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cd web && npx vitest run src/lib/whatsappExtractFields.test.ts`

- [ ] **Step 3: Implement the helper** using `mediaExtractFields`.

- [ ] **Step 4: Re-run — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/whatsappExtractFields.ts web/src/lib/whatsappExtractFields.test.ts web/src/lib/types.ts
git commit -m "$(cat <<'EOF'
feat(import): build WhatsApp extract extras without the iMessage password

Keep the Android key on its own field so a crypt hex is never stored
as an Apple backup password.
EOF
)"
```

---

### Task 4: Import form fields

**Files:**
- Modify: `web/src/screens/import/ImportFormFields.tsx`
- Modify: `web/src/screens/import/ImportFormFields.test.tsx`

**Interfaces:**
- Consumes: `whatsappCanImport`, `WHATSAPP_METHODS`, `WHATSAPP_SOURCE_ID`, show-* helpers, error constants
- Produces: when `isWhatsappMethod(source)`, source dropdown selectedKey is `WHATSAPP_SOURCE_ID`; Platform dropdown uses method ids; Android/iPhone fields match the spec table; Import uses `whatsappCanImport` instead of `Boolean(backupPath)`

New props (add to `ImportFormFieldsProps`):

```typescript
whatsappKey: string;
onWhatsappKeyChange: (value: string) => void;
showWhatsappKey: boolean;
onToggleWhatsappKey: () => void;
whatsappWa: string;
onWhatsappWaChange: (path: string) => void;
whatsappMedia: string;
onWhatsappMediaChange: (path: string) => void;
whatsappDb: string;
onWhatsappDbChange: (path: string) => void;
whatsappBusiness: boolean;
onWhatsappBusinessChange: (value: boolean) => void;
whatsappStats: WhatsappPathStats;
```

Labels (verbatim):

- Backup folder — required
- Decryption key — `required={whatsappCryptRequired(...)}` `optional={!required}`
- Contacts database — optional
- Media folder — optional (Android)
- Message database — optional (Android)
- WhatsApp Business — checkbox, unmarked

Hints:

- Android folder: `Folder that contains msgstore.db or msgstore.db.crypt12 / crypt14 / crypt15.`
- iPhone folder: `Path to the root of a device backup`
- Key: `Key file or crypt15 hex. Needed when the folder has an encrypted backup and no msgstore.db.`
- Contacts Android: `Leave empty if wa.db is in the backup folder.`
- Contacts iPhone: `Leave empty if ContactsV2.sqlite is in the backup.`
- Media: `Leave empty if the WhatsApp media folder is in the backup folder.`
- Message database: `Leave empty if msgstore.db is in the backup folder.`

Source `onSelectionChange`: if key is `WHATSAPP_SOURCE_ID`, call `onSourceChange(WHATSAPP_DEFAULT_METHOD)` (or the last WhatsApp method if the parent tracks it, same as iMessage). Platform `onSelectionChange`: only if `isWhatsappMethod(key)`.

Show `AttachmentFields` and `ContactsField` for WhatsApp (`showCompress` includes WhatsApp).

Decryption key: `PasswordField` so hex is hidden, same toggle pattern as iMessage encryption password. The value may be a file path or hex; typing a path is allowed.

- [ ] **Step 1: Extend `renderForm` defaults** with empty WhatsApp extras and `emptyWhatsappPathStats()`. Update the test that picks iMessage from `source: "whatsapp-android"` so the source list still finds **WhatsApp** then **iMessage**. Add tests:
  - source WhatsApp + Android: Platform options Android and iPhone; key visible; Business hidden; Attachments visible
  - source WhatsApp + iPhone: key hidden; Business checkbox visible; media/db hidden
  - crypt folder empty key: `Decryption key is required for an encrypted backup.`; Import disabled
  - folder path is a file: `Pick the backup folder.`

- [ ] **Step 2: Run `cd web && npx vitest run src/screens/import/ImportFormFields.test.tsx` — expect FAIL** (new props / missing UI)

- [ ] **Step 3: Implement the WhatsApp branch** next to the iMessage branch. Do not leave WhatsApp on the generic “Backup path” else arm.

- [ ] **Step 4: Re-run — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/import/ImportFormFields.tsx web/src/screens/import/ImportFormFields.test.tsx
git commit -m "$(cat <<'EOF'
feat(import): show WhatsApp Platform fields for Android and iPhone

Put crypt key, contacts, media, and Business on the form that already
has a single WhatsApp source row.
EOF
)"
```

---

### Task 5: Screen state, path probe, remembered extras

**Files:**
- Modify: `web/src/screens/ImportScreen.tsx`
- Modify: `web/src/lib/system-settings.ts`
- Modify: `web/src/lib/system-settings.test.ts`
- Modify: `web/src/screens/import/useImportJob.ts`

**Interfaces:**
- Consumes: `invokePathStat`, `whatsappExtractFields`, `isWhatsappMethod`
- Produces: last WhatsApp method remembered across source switches (mirror `lastImessageMethodRef`); probe backup folder + optional paths; set `hasMsgstoreDb` / `cryptName` from `invokePathStat` on `folder/msgstore.db` and each `WHATSAPP_CRYPT_NAMES` entry; persist extras `whatsappWa` / `whatsappMedia` / `whatsappDb` per method id; never persist `whatsappKey`

Extend `ImporterExtraRow` with optional `whatsappWa`, `whatsappMedia`, `whatsappDb`. Extend `loadRememberedImportPaths` / `setImporterExtraPath` (or add WhatsApp-specific setters) so those three restore after a source change.

Path probe (Tauri only, debounce like iMessage):

```typescript
const root = backupPath.trim();
const msgstore = await probePath(root ? `${root}/msgstore.db` : "");
const cryptHits = await Promise.all(
  WHATSAPP_CRYPT_NAMES.map(async (name) => {
    const stat = await probePath(root ? `${root}/${name}` : "");
    return stat?.exists && stat.isFile ? name : null;
  }),
);
```

`cryptName` is the first non-null crypt hit. `hasMsgstoreDb` is `msgstore?.exists && msgstore.isFile`.

`useImportJob` `invokeExtract`: add

```typescript
...(isWhatsappMethod(form.source)
  ? whatsappExtractFields({
      source: form.source,
      attachmentMedia: form.attachmentMedia,
      maxResolution: form.maxResolution,
      maxFps: form.maxFps,
      minSizeMb: form.minSizeMb,
      key: form.whatsappKey,
      wa: form.whatsappWa,
      media: form.whatsappMedia,
      db: form.whatsappDb,
      business: form.whatsappBusiness,
    })
  : {}),
```

Extend `ImportJobFormValues` with those WhatsApp fields.

`invokeExtract` in `tauri.ts` must forward:

```typescript
whatsappKey: config.whatsapp_key ?? null,
whatsappWa: config.whatsapp_wa ?? null,
whatsappMedia: config.whatsapp_media ?? null,
whatsappDb: config.whatsapp_db ?? null,
whatsappBusiness: config.whatsapp_business ?? null,
```

- [ ] **Step 1: Add a system-settings test** that `setImporterPath("whatsapp-ios", "/backups/iphone")` still loads after the source list is one row, and that extra `whatsappWa` restores for `whatsapp-android` and is not mixed with `appleContacts`.

- [ ] **Step 2: Run `cd web && npx vitest run src/lib/system-settings.test.ts` — expect FAIL** on new extras.

- [ ] **Step 3: Implement extras + ImportScreen wiring + useImportJob + tauri.ts**

When the source dropdown sends `WHATSAPP_SOURCE_ID`, set source to `lastWhatsappMethodRef.current` (default Android).

- [ ] **Step 4: Run `cd web && npm test` — expect PASS** for the files this task touched. Fix any `ImportFormFields` prop holes in other tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/screens/ImportScreen.tsx web/src/lib/system-settings.ts \
        web/src/lib/system-settings.test.ts web/src/screens/import/useImportJob.ts \
        web/src/lib/tauri.ts
git commit -m "$(cat <<'EOF'
feat(import): probe WhatsApp folders and remember optional paths

Detect crypt files next to msgstore.db so the key star is real, and
keep Android and iPhone remembered folders on their old ids.
EOF
)"
```

---

### Task 6: Tauri extract fills `WhatsappConfig`

**Files:**
- Modify: `src-tauri/src/commands/extract.rs`

**Interfaces:**
- Consumes: new optional fields on `ExtractArgs` (`whatsapp_key`, `whatsapp_wa`, `whatsapp_media`, `whatsapp_db`, `whatsapp_business`)
- Produces: `whatsapp-android` / `whatsapp-ios` `SourceConfig::Whatsapp` with platform plus those fields; iPhone sets `backup: Some(PathBuf::from(path))`; Android leaves `backup` unset so the converter can find a crypt file; `inputs` stays `vec![PathBuf::from(path)]`

`ExtractOptions` gains the same five fields. `test_options` initializes them empty/false.

- [ ] **Step 1: Write failing tests** in the existing `extract.rs` tests module:

```rust
#[test]
fn whatsapp_android_forwards_key_and_optional_paths() {
    let mut options = test_options(Vec::new());
    options.whatsapp_key = "deadbeef".into();
    options.whatsapp_wa = "/tmp/wa.db".into();
    options.whatsapp_media = "/tmp/WhatsApp".into();
    options.whatsapp_db = "/tmp/msgstore.db".into();
    options.whatsapp_business = true;
    let config = build_exporter_config(
        "whatsapp-android",
        "/tmp/android-dump",
        "/tmp/out",
        &options,
    )
    .unwrap();
    match config.source {
        SourceConfig::Whatsapp(wa) => {
            assert_eq!(wa.platform, Some(WhatsappPlatform::Android));
            assert_eq!(wa.key.as_deref(), Some("deadbeef"));
            assert_eq!(wa.wa.as_deref(), Some(std::path::Path::new("/tmp/wa.db")));
            assert!(wa.backup.is_none());
            assert!(!wa.business);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn whatsapp_ios_sets_backup_from_folder_and_business() {
    let mut options = test_options(Vec::new());
    options.whatsapp_business = true;
    let config =
        build_exporter_config("whatsapp-ios", "/tmp/ios-backup", "/tmp/out", &options).unwrap();
    match config.source {
        SourceConfig::Whatsapp(wa) => {
            assert_eq!(wa.platform, Some(WhatsappPlatform::Ios));
            assert_eq!(
                wa.backup.as_deref(),
                Some(std::path::Path::new("/tmp/ios-backup"))
            );
            assert!(wa.business);
            assert!(wa.key.is_none());
        }
        other => panic!("{other:?}"),
    }
}
```

Android must **not** set `business` even if the option is true (form will not send it; still clamp in `build_exporter_config`).

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- extract::tests::whatsapp_android_forwards_key_and_optional_paths extract::tests::whatsapp_ios_sets_backup_from_folder_and_business`

Expected: FAIL (fields missing / `WhatsappConfig` still default).

- [ ] **Step 3: Plumb `ExtractArgs` → `ExtractOptions` → `WhatsappConfig`.** Use `optional_trimmed` for paths. iPhone: `backup: Some(PathBuf::from(path))`. Android: `backup: None`.

- [ ] **Step 4: Re-run — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/extract.rs
git commit -m "$(cat <<'EOF'
feat(tauri): pass WhatsApp key, contacts, and iPhone backup path

The extract command used to set only the platform, so crypt backups
and Finder folders never reached wtsexporter.
EOF
)"
```

---

### Task 7: Converter finds Android crypt files

**Files:**
- Modify: `crates/exporters/whatsapp-exporter/src/wtsexporter.rs`

**Interfaces:**
- Consumes: `WtsexporterArgs.platform`, `input`, existing `backup` / `key`
- Produces: `pub(crate) fn android_crypt_backup(search: &Path) -> Option<PathBuf>` — `None` if `search/msgstore.db` is a file; else first existing crypt name. `resolve_forwarded_paths` uses this when `platform == Android` and `args.backup` is `None`. Do not pass `-k` when backup stays `None` (strip key in that case).

Never pass `--wab`, `--call-db`, `-e`, or `-c`.

- [ ] **Step 1: Add tests** at the bottom of `wtsexporter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::android_crypt_backup;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn prefers_msgstore_db_over_crypt() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("msgstore.db"), b"db").unwrap();
        fs::write(dir.path().join("msgstore.db.crypt15"), b"crypt").unwrap();
        assert_eq!(android_crypt_backup(dir.path()), None);
    }

    #[test]
    fn finds_crypt15_when_msgstore_missing() {
        let dir = tempdir().unwrap();
        let crypt = dir.path().join("msgstore.db.crypt15");
        fs::write(&crypt, b"crypt").unwrap();
        assert_eq!(android_crypt_backup(dir.path()).as_deref(), Some(crypt.as_path()));
    }
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p whatsapp-exporter --lib android_crypt`

Expected: FAIL (`android_crypt_backup` missing).

- [ ] **Step 3: Implement `android_crypt_backup` and wire it into `resolve_forwarded_paths`.** If no backup is forwarded, set `key` to `None` even if `args.key` is set.

- [ ] **Step 4: Re-run — expect PASS.** Also run `cargo test -p whatsapp-exporter`.

- [ ] **Step 5: Commit**

```bash
git add crates/exporters/whatsapp-exporter/src/wtsexporter.rs
git commit -m "$(cat <<'EOF'
feat(whatsapp): pass crypt files in the backup folder as -b

A folder with only msgstore.db.crypt15 must reach wtsexporter without
a second picker, and a decrypted msgstore.db in the same folder wins.
EOF
)"
```

---

### Task 8: Docs and changelog

**Files:**
- Modify: `docs/src/content/docs/vault/user/import-from-a-backup.md`
- Modify: `docs/src/content/docs/vault/user/prepare-a-backup/android-whatsapp.md`
- Modify: `docs/src/content/docs/vault/user/prepare-a-backup/iphone-whatsapp.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: locked field table and docs bullets in the spec
- Produces: User Guide that matches the form

Import table rows become:

| Source in the app | Typical files |
|---|---|
| **WhatsApp** → **Platform:** **Android** | Folder with `msgstore.db` or `msgstore.db.crypt*` plus key |
| **WhatsApp** → **Platform:** **iPhone** | iPhone backup that includes WhatsApp |

Add a **WhatsApp fields** subsection modeled on **iMessage fields**: stars, optional contacts/media/db, Android key, iPhone Business checkbox. State that the key is not the Apple backup password.

Android prepare page next step: **Import**, source **WhatsApp**, Platform **Android**.

iPhone prepare page: remove “If the backup is encrypted, you need the password. The desktop app does not store it.” Replace with: WhatsApp-on-iPhone Import points at the Finder/iTunes backup folder. It does not ask for the Apple backup password. Encrypted device backups are a `wtsexporter` limitation, not a field on this form.

CHANGELOG under `[Unreleased]` / `### Changed`:

```markdown
- 2026-08-26: Import lists one **WhatsApp** source with Platform Android or iPhone. Android can decrypt a crypt12/14/15 file in the backup folder with a key; iPhone forwards the Finder backup as `-b`. Optional contacts, media, and message-database paths stay empty when those files already sit in the folder.
```

- [ ] **Step 1: Edit the three docs pages and CHANGELOG**

- [ ] **Step 2: Check docs**

Run: `cd docs && npm run check && npm run build`

Expected: PASS.

- [ ] **Step 3: Run web tests once more**

Run: `cd web && npm test`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add docs/src/content/docs/vault/user/import-from-a-backup.md \
        docs/src/content/docs/vault/user/prepare-a-backup/android-whatsapp.md \
        docs/src/content/docs/vault/user/prepare-a-backup/iphone-whatsapp.md \
        CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs(import): describe the unified WhatsApp Platform form

The User Guide still named two WhatsApp sources and told iPhone
users to type an Apple backup password that the form does not have.
EOF
)"
```

---

## Spec coverage

| Spec section | Task |
|---|---|
| One WhatsApp source + Platform Android/iPhone | 1, 2, 4 |
| Internal ids / remembered paths | 2, 5 |
| Field table + stars | 1, 4 |
| Skip `-e` / `--call-db` / `--wab` / `--move-media` | 6, 7 (never pass) |
| Android crypt lookup + key rules | 1, 5, 7 |
| iPhone folder as `-b` | 6 |
| Extract extras + media/contacts on WhatsApp | 3, 5, 6 |
| Form error catalog | 1, 4 |
| Docs + no Apple password on WhatsApp iPhone | 8 |
| No key persistence | 5 |
| CLI `--input` crypt behavior | 7 |
