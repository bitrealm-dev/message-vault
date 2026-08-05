---
title: Troubleshooting
description: Fix common problems with the Message Exporters desktop app.
---

## The app will not start

**Windows SmartScreen or "unrecognized app" warning.** Click **More info** and then **Run anyway**. The app is not signed with a code-signing certificate, so Windows flags it on first launch. You only need to allow it once.

**macOS Gatekeeper or "cannot be opened" warning.** Go to **System Settings → Privacy & Security** and click **Open Anyway** next to the message about the app. Alternatively, right-click the app in Finder and choose **Open**.

**The archive was not extracted.** Running the app from inside the downloaded `.zip` or `.tgz` will fail. Extract the entire archive to a permanent folder and keep every file together — the `lib/` and `cli/` folders must stay next to the app.

**Helper programs moved or deleted.** The app looks for `ffmpeg` / `ffprobe` under `lib/` and `wtsexporter` under `cli/`, next to the app binary. If you moved those folders, the app cannot find them. Extract the archive fresh and keep the layout intact.

## Extraction fails

**Wrong platform auto-detection.** If the app guesses the wrong platform for an iPhone backup (iOS vs macOS), use the **Platform** dropdown to set it explicitly. The same applies to WhatsApp: choose Android or iOS in the form.

**Encrypted backup password is wrong.** Double-check the backup password. The app cannot extract from an encrypted iPhone backup without the correct password.

**Wrong WhatsApp decryption key.** The key must be the full 64-character hex string. If your backup uses a key file instead, pass the file path. Re-export the key from your WhatsApp backup tool if the value is uncertain.

**wtsexporter not found.** The WhatsApp path needs a Python helper. It should be in `cli/wtsexporter` next to the app binary. If you are building from source, install it with pip:

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
```

Then set `WTSEXPORTER` to the full path or add it to your `PATH`.

**Cancellation does not stop immediately.** The app uses cooperative cancellation. It cannot stop the external `wtsexporter` process mid-run during WhatsApp extraction. Wait for it to finish or kill the process manually.

## Media problems

**ffmpeg or ffprobe not found.** The **Convert** and **Compress** attachment modes need FFmpeg. The app looks for `lib/ffmpeg` and `lib/ffprobe` next to the binary. If you unzipped the archive and kept the folders together, they are already there. If you are building from source, install ffmpeg from your package manager and make sure it is on `PATH`.

**Conversion produces no output or low-quality results.** Check the **Compress options** in the advanced section. The defaults (1080p, 30 fps, 20 MB minimum) are conservative. Raise or lower them for your needs. The log tab shows which files were converted and which were skipped.

## Output problems

**"Input and output must differ" error (Format tab).** When converting between formats, the output directory must be different from the input directory. Choose a new empty folder.

**Conversation names look unexpected.** Files named `group_...` or ending with `__whatsapp` are normal. The tool uses these stem suffixes to distinguish group chats and WhatsApp conversations from other message types.

**Obfuscation preview looks wrong.** If you enabled obfuscation and the results do not look as expected, check the seed value. An empty seed generates a random one at run time — each run produces different pseudonyms. Set an explicit 8-character hex seed for reproducible results.

**Some messages are missing from a rescue import.** The limited rescue formats (GO SMS Pro, iMazing, OpenExtract, SMS Backup+) cannot preserve everything the original backup contained. Each of those guides has a "Known limitations" section. If the source format did not store the data, the exporter cannot recover it.

## Getting help

If you cannot find an answer here, open an issue on GitHub:

- [Message Exporters issues](https://github.com/bitrealm-dev/message-vault-io/issues) — for the desktop app, exporters, or CLI tools.
- [Message Vault issues](https://github.com/bitrealm-dev/message-vault-rs/issues) — for the vault server and web UI.

Include:
- Your operating system and version
- The backup source you are using
- The Log tab output from the failed run (copy the relevant lines)
- Any error messages shown in the app

Do not include passwords, vault keys, phone numbers, or message content in a public issue.
