# Third-party notices

This release archive bundles third-party binaries under `lib/` and `cli/`.
Keep the extracted folder together so helper discovery works. License texts for
those tools live in this `licenses/` directory.

## wtsexporter

- Project: [KnugiHK/WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter)
- Version: `0.13.0`
- Role: extracts WhatsApp databases for `whatsapp-exporter` / the GUI WhatsApp source
- Location in archive: `cli/wtsexporter`
- License: MIT (see `THIRD_PARTY_WTSEXPORTER.LICENSE` in this directory)

## ffmpeg / ffprobe

- Build source: [eugeneware/ffmpeg-static](https://github.com/eugeneware/ffmpeg-static) tag `b6.1.1`
- Binary reports: FFmpeg / FFprobe `7.0.2-static` (John Van Sickle / platform build provenance in `THIRD_PARTY_FFMPEG.LICENSE`)
- Role: **Convert** and **Convert & compress** attachment modes
- Location in archive: `lib/ffmpeg`, `lib/ffprobe`
- License: GPL (see `THIRD_PARTY_FFMPEG.LICENSE` in this directory)

The Message Vault project itself remains under the terms in `LICENSE`, except
where a workspace crate states otherwise (for example `imessage-ir-exporter` is
GPL-3.0-or-later via its dependencies).
