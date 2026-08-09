# Import Messages (iOS) — Tauri screen

**Date**: 2026-08-08  
**Status**: approved for implementation

## Context

The Tauri desktop Import screen (`web/src/screens/ImportScreen.tsx`) currently collects only a backup source, backup path, and optional contacts file. The iMessage iOS wireframes require a fuller form: encryption password, attachment media modes (with compress options), vault-based contact name modes, and a collapsible Message Filtering section. Those controls must drive the extract command and the vault push path.

Slint GUI is out of scope. The offline Extract tab is left on its thin form for this pass.

## Screen layout (imessage-ios)

1. Title **Import Messages**, subtitle **Select your messages.**
2. **Import Format** dropdown + **Need help?** (opens public docs for iMessage/iOS backup).
3. **iPhone Backup Directory** + Browse.
4. **Encryption password (optional)**.
5. **Message Attachments**: Copy / Convert / Compress & Convert / Skip.  
   When Compress & Convert: indented Target resolution, Max FPS, Minimum file size (MB).
6. **Contacts** name mode (main section, after attachments):
   - Fill in missing names from vault contacts (default)
   - Overwrite all import names with vault contacts
7. Horizontal divider.
8. **Message Filtering** (collapsed by default): participant filter, start date, end date (exclusive), obfuscate.
9. **Import** / **Cancel** while running.

iOS-specific fields stay in the main section (no Advanced importer block this pass). Other sources keep path-only fields. Contacts file picker and conflict-review table are removed.

## Extract wiring

`ExtractConfig` gains optional fields: backup password, attachment media, compress options, conversation filter, start/end dates, obfuscate. The Tauri `extract` command maps them into `ExporterConfig` (`AppleConfig` for iOS, `MediaConfig`, `DateRange`, `ObfuscateConfig`). Output remains JSONL for the import pipeline. Omitting optional fields preserves today’s bare Extract behavior.

## Contact name mode

Enum: `fill_missing` | `overwrite`.

- Passed on push → `POST /v1/import` (query param).
- Server looks up account contacts by handle when applying participant/sender display names:
  - **fill_missing**: set name only when the import name is empty and a vault contact name exists.
  - **overwrite**: when a vault contact name exists, always use it.
- No matching contact → leave the import name unchanged.

This is separate from loading a contacts file (`overwrite_contacts`).

## Out of scope

- Slint
- Per-source Advanced sections above attachments
- Rebuilding Extract.tsx to the same wireframe
- Contacts file merge / conflict review
- Processing Options (continue on error / force) UI
