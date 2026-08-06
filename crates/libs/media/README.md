# message-media

Post-process attachment media under a converter export directory (`attachments/` + CSV `attachments_json` paths).

## Modes

| Mode | Behavior |
|------|----------|
| `disabled` | Do not write attachment files (exporter flag; post-process no-op) |
| `clone` | Leave files as exported (post-process no-op) |
| `convert` | Standardize images→`.jpg`, videos→`.mp4`, audio→`.mp3` (`.gif` left unchanged) |
| `compress` | Size-oriented re-encode; video options for max resolution / fps / min size / skip-efficient (`.gif` left unchanged) |

Intermediate `*.msgmedia.tmp.*` files are deleted after each file and swept from `attachments/` at the start and end of a run.

Requires **ffmpeg** and **ffprobe** for convert/compress (beside the running binary, in `MESSAGE_VAULT_IO_BIN`, or on `PATH`). Release ZIPs bundle both.

Used by `go-sms-pro-exporter`, `sms-backup-restore-exporter`, `sms-backup-plus-exporter`, and the GUI (iPhone convert/compress post-step).
