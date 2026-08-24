//! Generate browser-friendly derived media under `assets_converted/`.
//!
//! Keeps originals intact, writes content-addressed JPEG/MP4/MP3 blobs, and
//! updates `attachments.derived_*`. Requires `ffmpeg` / `ffprobe` on PATH for
//! conversions (images, video, audio).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sqlx::AnyConnection;
use tempfile::TempDir;

use crate::config::Config;
use crate::db::{engine, schema};
use crate::media_tools::{
    self, JPEG_MIN_BYTES, MP3_MIN_BYTES, MP4_MIN_BYTES, MediaKind, ext_of, path_str,
    probe_video_efficient,
};

/// Options for one derived-media processing pass.
#[derive(Debug, Clone, Default)]
pub struct ProcessAssetsOptions {
    /// Re-convert even when a browser preview already exists.
    pub force: bool,
    /// Convert and log without writing files or updating the database.
    pub dry_run: bool,
    /// Skip image conversion.
    pub skip_image: bool,
    /// Skip video conversion.
    pub skip_video: bool,
    /// Skip audio conversion.
    pub skip_audio: bool,
    /// Override DB path from config.
    pub db: Option<PathBuf>,
    /// Only process this source id.
    pub source: Option<String>,
}

/// Counts reported by one derived-media processing pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProcessAssetsStats {
    /// Attachments examined.
    pub scanned: u64,
    /// Browser previews written (JPEG/MP4/MP3).
    pub derived: u64,
    /// Attachments left as-is (already converted, non-media, or small JPEG).
    pub skipped: u64,
    /// Conversions that failed.
    pub errors: u64,
}

#[derive(Debug, Clone)]
struct DerivedBlob {
    sha256: String,
    assets_path: String,
    mime_type: String,
}

#[derive(Debug)]
struct AssetRow {
    sha256: String,
    assets_path: String,
    mime_type: Option<String>,
    derived_assets_path: Option<String>,
    /// Attachment file name from the export (`attachments.original_name`).
    original_name: Option<String>,
    /// Attachment path inside the export (`attachments.path`).
    source_path: Option<String>,
}

impl AssetRow {
    /// Extension sources to fall back on when the stored blob has none.
    fn name_hints(&self) -> [Option<&str>; 2] {
        [self.original_name.as_deref(), self.source_path.as_deref()]
    }
}

/// Run derived-media conversion for every account/source in the vault.
///
/// # Errors
///
/// Returns an error when the database is missing, a conversion tool fails, or
/// a derived file cannot be written.
pub async fn run(cfg: &Config, opts: &ProcessAssetsOptions) -> Result<ProcessAssetsStats> {
    let db_path = opts.db.as_ref().unwrap_or(&cfg.paths.db);
    if !db_path.is_file() {
        bail!("database not found: {}", db_path.display());
    }

    let pool = engine::open_pool_for_path(db_path)
        .await
        .with_context(|| format!("open database {}", db_path.display()))?;
    let mut conn = pool.acquire().await?;
    schema::ensure_vault_schema(&mut conn).await?;

    let account_ids = list_account_ids(&mut conn, &cfg.paths.data_dir).await?;
    if account_ids.is_empty() {
        bail!("no accounts found — create an account or run reset-demo first");
    }

    let work = TempDir::new().context("create temp dir for derived media")?;
    let mut stats = ProcessAssetsStats::default();

    for account_id in &account_ids {
        let mut source_ids = discover_source_ids(
            &mut conn,
            account_id,
            &cfg.paths.data_dir,
            &cfg.paths.assets_dir,
        )
        .await?;
        if let Some(filter) = opts.source.as_deref() {
            let filter = filter.trim();
            source_ids.retain(|id| id == filter);
            if source_ids.is_empty() {
                bail!("unknown source '{filter}' for account {account_id}");
            }
        }
        if source_ids.is_empty() {
            eprintln!("account {account_id}: no sources found — skip");
            continue;
        }

        for source_id in source_ids {
            let assets_dir = cfg.paths.assets_dir_for_account(account_id, &source_id);
            let converted_dir = cfg
                .paths
                .assets_converted_dir_for_account(account_id, &source_id);
            println!(
                "account {account_id} source {source_id}: assets={}",
                assets_dir.display()
            );
            if !assets_dir.is_dir() {
                eprintln!("  skip — assets dir missing");
                continue;
            }
            let cleaned = cleanup_incoming_parts(&assets_dir, opts.dry_run)?;
            if cleaned > 0 {
                println!("  cleaned {cleaned} leftover .part upload temp(s) under .incoming/");
            }
            fs::create_dir_all(&converted_dir)
                .with_context(|| format!("create converted dir {}", converted_dir.display()))?;

            let rows = list_attachments(&mut conn, account_id, &source_id).await?;
            for row in rows {
                stats.scanned += 1;
                match process_one(ProcessOneArgs {
                    conn: &mut conn,
                    opts,
                    work_dir: work.path(),
                    account_id,
                    source_id: &source_id,
                    assets_dir: &assets_dir,
                    converted_dir: &converted_dir,
                    row: &row,
                })
                .await
                {
                    Ok(Outcome::Derived) => stats.derived += 1,
                    Ok(Outcome::Skipped) => stats.skipped += 1,
                    Err(err) => {
                        stats.errors += 1;
                        eprintln!(
                            "failed {account_id}/{source_id}/{}: {err:#}",
                            row.assets_path
                        );
                    }
                }
            }
        }
    }

    println!(
        "done: scanned={} converted_for_web={} left_as_is={} conversion_failures={}{}",
        stats.scanned,
        stats.derived,
        stats.skipped,
        stats.errors,
        if opts.dry_run { " (dry-run)" } else { "" }
    );
    Ok(stats)
}

enum Outcome {
    Derived,
    Skipped,
}

struct ProcessOneArgs<'a> {
    conn: &'a mut AnyConnection,
    opts: &'a ProcessAssetsOptions,
    work_dir: &'a Path,
    account_id: &'a str,
    source_id: &'a str,
    assets_dir: &'a Path,
    converted_dir: &'a Path,
    row: &'a AssetRow,
}

async fn process_one(args: ProcessOneArgs<'_>) -> Result<Outcome> {
    let ProcessOneArgs {
        conn,
        opts,
        work_dir,
        account_id,
        source_id,
        assets_dir,
        converted_dir,
        row,
    } = args;
    // Incomplete transfers / aborted uploads — never hand these to ffmpeg.
    if is_part_path(&row.assets_path) {
        let source_path = assets_dir.join(&row.assets_path);
        if source_path.is_file() {
            if opts.dry_run {
                println!(
                    "[dry-run] would remove incomplete {account_id}/{source_id}/{}",
                    row.assets_path
                );
            } else {
                fs::remove_file(&source_path)
                    .with_context(|| format!("remove incomplete {}", source_path.display()))?;
                println!(
                    "removed incomplete {account_id}/{source_id}/{}",
                    row.assets_path
                );
            }
        }
        return Ok(Outcome::Skipped);
    }

    let kind = kind_of(
        &row.assets_path,
        row.mime_type.as_deref(),
        &row.name_hints(),
    );
    match kind {
        MediaKind::Image if opts.skip_image => return Ok(Outcome::Skipped),
        MediaKind::Video if opts.skip_video => return Ok(Outcome::Skipped),
        MediaKind::Audio if opts.skip_audio => return Ok(Outcome::Skipped),
        MediaKind::Other => return Ok(Outcome::Skipped),
        _ => {}
    }

    if should_skip_existing(
        opts.force,
        row.derived_assets_path.as_deref(),
        converted_dir,
    ) {
        return Ok(Outcome::Skipped);
    }

    let source_path = assets_dir.join(&row.assets_path);
    if !source_path.is_file() {
        bail!(
            "missing original: {account_id}/{source_id}/{}",
            row.assets_path
        );
    }

    let blob = match kind {
        MediaKind::Image => {
            let Some(jpeg) = derive_image(&source_path)? else {
                return Ok(Outcome::Skipped);
            };
            if opts.dry_run {
                println!(
                    "[dry-run] image {account_id}/{source_id}/{} -> jpeg {} bytes",
                    row.assets_path,
                    jpeg.len()
                );
                return Ok(Outcome::Derived);
            }
            store_derived_bytes(converted_dir, &jpeg, ".jpg")?
        }
        MediaKind::Video => {
            let Some(out) = derive_video(&source_path, work_dir)? else {
                return Ok(Outcome::Skipped);
            };
            if opts.dry_run {
                println!(
                    "[dry-run] video {account_id}/{source_id}/{} -> mp4",
                    row.assets_path
                );
                let _ = fs::remove_file(&out);
                return Ok(Outcome::Derived);
            }
            let blob = store_derived_file(converted_dir, &out, ".mp4")?;
            let _ = fs::remove_file(&out);
            blob
        }
        MediaKind::Audio => {
            let Some(out) = derive_audio(&source_path, work_dir)? else {
                return Ok(Outcome::Skipped);
            };
            if opts.dry_run {
                println!(
                    "[dry-run] audio {account_id}/{source_id}/{} -> mp3",
                    row.assets_path
                );
                let _ = fs::remove_file(&out);
                return Ok(Outcome::Derived);
            }
            let blob = store_derived_file(converted_dir, &out, ".mp3")?;
            let _ = fs::remove_file(&out);
            blob
        }
        MediaKind::Other => return Ok(Outcome::Skipped),
    };

    update_derived(conn, account_id, source_id, &row.sha256, &blob).await?;
    println!(
        "{account_id}/{source_id}/{} -> {}",
        row.assets_path, blob.assets_path
    );
    Ok(Outcome::Derived)
}

async fn list_account_ids(conn: &mut AnyConnection, data_dir: &Path) -> Result<Vec<String>> {
    // Engine-branched: sqlite_master does not exist on Postgres.
    let mut ids = Vec::new();
    if schema::table_exists(conn, "accounts").await? {
        let rows = sqlx::query_scalar::<_, String>("SELECT id FROM accounts ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
        ids = rows;
    }
    if ids.is_empty() && data_dir.is_dir() {
        for entry in fs::read_dir(data_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                ids.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        ids.sort();
    }
    Ok(ids)
}

async fn discover_source_ids(
    conn: &mut AnyConnection,
    account_id: &str,
    data_dir: &Path,
    assets_name: &str,
) -> Result<Vec<String>> {
    let mut ids = std::collections::BTreeSet::new();
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT m.source
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE c.account_id = $1
          AND m.source IS NOT NULL
          AND TRIM(m.source) != ''
        ORDER BY m.source
        "#,
    )
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;
    for s in rows {
        let t = s.trim();
        if !t.is_empty() {
            ids.insert(t.to_string());
        }
    }

    let account_root = data_dir.join(account_id);
    if account_root.is_dir() {
        for entry in fs::read_dir(&account_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if account_root.join(&name).join(assets_name).is_dir() {
                ids.insert(name);
            }
        }
    }
    Ok(ids.into_iter().collect())
}

async fn list_attachments(
    conn: &mut AnyConnection,
    account_id: &str,
    source_id: &str,
) -> Result<Vec<AssetRow>> {
    // One row per stored blob. Several messages can share a blob under different
    // names, and only one derived file per blob is ever produced, so collapse
    // those rows and keep any name that could identify the media type.
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            a.sha256,
            a.assets_path,
            MAX(a.mime_type),
            MAX(a.derived_assets_path),
            MAX(a.original_name),
            MAX(a.path)
        FROM attachments a
        JOIN messages m ON m.id = a.message_id
        JOIN conversations c ON c.id = m.conversation_id
        WHERE m.source = $1
          AND c.account_id = $2
          AND a.sha256 IS NOT NULL AND a.sha256 != ''
          AND a.assets_path IS NOT NULL AND a.assets_path != ''
        GROUP BY a.sha256, a.assets_path
        ORDER BY a.sha256
        "#,
    )
    .bind(source_id)
    .bind(account_id)
    .fetch_all(&mut *conn)
    .await?;
    let out = rows
        .into_iter()
        .map(
            |(sha256, assets_path, mime_type, derived_assets_path, original_name, source_path)| {
                AssetRow {
                    sha256,
                    assets_path,
                    mime_type,
                    derived_assets_path,
                    original_name,
                    source_path,
                }
            },
        )
        .collect();
    Ok(out)
}

async fn update_derived(
    conn: &mut AnyConnection,
    account_id: &str,
    source_id: &str,
    original_sha: &str,
    blob: &DerivedBlob,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE attachments
        SET derived_sha256 = $1, derived_assets_path = $2, derived_mime_type = $3
        WHERE sha256 = $4
          AND message_id IN (
            SELECT m.id FROM messages m
            JOIN conversations c ON c.id = m.conversation_id
            WHERE m.source = $5 AND c.account_id = $6
          )
        "#,
    )
    .bind(&blob.sha256)
    .bind(&blob.assets_path)
    .bind(&blob.mime_type)
    .bind(original_sha)
    .bind(source_id)
    .bind(account_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Incomplete iMessage/SMS transfers and aborted vault uploads use a `.part` suffix.
fn is_part_path(path: &str) -> bool {
    has_part_extension(Path::new(path))
}

fn has_part_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    ext.eq_ignore_ascii_case("part")
}

/// Max age for abandoned multipart upload sessions under `.incoming/{sha}/{upload_id}/`.
const STALE_UPLOAD_SESSION_SECS: u64 = 24 * 60 * 60;

/// Remove stale `{sha}-*.part` temps and abandoned multipart session dirs under `.incoming/`.
fn cleanup_incoming_parts(assets_dir: &Path, dry_run: bool) -> Result<u64> {
    let incoming = assets_dir.join(".incoming");
    if !incoming.is_dir() {
        return Ok(0);
    }
    let mut removed = 0u64;
    let now = std::time::SystemTime::now();
    for entry in fs::read_dir(&incoming).with_context(|| format!("read {}", incoming.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if !has_part_extension(&path) {
                continue;
            }
            if dry_run {
                println!("[dry-run] would remove {}", path.display());
                removed += 1;
                continue;
            }
            fs::remove_file(&path)
                .with_context(|| format!("remove leftover {}", path.display()))?;
            removed += 1;
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        // Multipart staging: `.incoming/{sha256}/{upload_id}/`
        for session_ent in
            fs::read_dir(&path).with_context(|| format!("read {}", path.display()))?
        {
            let session_ent = session_ent?;
            let session = session_ent.path();
            if !session.is_dir() {
                continue;
            }
            if !upload_session_is_stale(&session, now)? {
                continue;
            }
            if dry_run {
                println!(
                    "[dry-run] would remove stale upload session {}",
                    session.display()
                );
                removed += 1;
                continue;
            }
            fs::remove_dir_all(&session)
                .with_context(|| format!("remove stale upload session {}", session.display()))?;
            removed += 1;
        }
        // Drop empty sha parent dirs.
        if path.is_dir() && fs::read_dir(&path)?.next().is_none() {
            if dry_run {
                println!("[dry-run] would remove empty {}", path.display());
            } else {
                let _ = fs::remove_dir(&path);
            }
        }
    }
    Ok(removed)
}

fn upload_session_is_stale(session: &Path, now: std::time::SystemTime) -> Result<bool> {
    let manifest = session.join("manifest.json");
    let meta = if manifest.is_file() {
        fs::metadata(&manifest)?
    } else {
        fs::metadata(session)?
    };
    let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
    let age = now.duration_since(modified).unwrap_or_default();
    Ok(age.as_secs() >= STALE_UPLOAD_SESSION_SECS)
}

/// Classify a stored file, falling back to the names the export supplied.
///
/// Stored files are named `<folder>/<sha256>` with no extension, so a row whose
/// `mime_type` is missing (older imports, or a source that declared nothing)
/// would otherwise classify as `Other` and never be converted for the browser.
/// `name_hints` are the attachment's `original_name` and original export `path`,
/// used only when the stored path and the declared MIME say nothing.
fn kind_of(assets_path: &str, mime: Option<&str>, name_hints: &[Option<&str>]) -> MediaKind {
    if is_part_path(assets_path) {
        return MediaKind::Other;
    }
    let declared = nonempty_mime(mime);
    let kind = media_tools::kind_of(Path::new(assets_path), declared);
    // A stored path that already identifies the file wins, and so does a
    // declared MIME — including `image/gif` and `.gif`, which stay unconverted.
    if kind != MediaKind::Other || declared.is_some() || ext_of(Path::new(assets_path)) == ".gif" {
        return kind;
    }
    for hint in name_hints.iter().flatten() {
        let hinted = media_tools::kind_of(Path::new(hint), None);
        if hinted != MediaKind::Other {
            return hinted;
        }
    }
    MediaKind::Other
}

fn nonempty_mime(mime: Option<&str>) -> Option<&str> {
    let raw = mime?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn derive_image(source_path: &Path) -> Result<Option<Vec<u8>>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    let is_jpeg = ext == ".jpg" || ext == ".jpeg";
    if is_jpeg && size <= JPEG_MIN_BYTES {
        return Ok(None);
    }
    if !media_tools::tool_on_path("ffmpeg") {
        bail!("ffmpeg required for image derived media");
    }
    let tmp = tempfile::Builder::new()
        .suffix(".jpg")
        .tempfile()
        .context("temp jpeg")?;
    let tmp_path = tmp.path().to_path_buf();
    // High-quality still (`-q:v 2` ≈ quality ~85 intent); autorotate is ffmpeg default.
    media_tools::run_ffmpeg(
        &[
            "-i",
            path_str(source_path)?,
            "-frames:v",
            "1",
            "-update",
            "1",
            "-q:v",
            "2",
            path_str(&tmp_path)?,
        ],
        Some(&tmp_path),
    )?;
    let mut buf = Vec::new();
    File::open(&tmp_path)?.read_to_end(&mut buf)?;
    Ok(Some(buf))
}

fn derive_video(source_path: &Path, work_dir: &Path) -> Result<Option<PathBuf>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    if ext == ".mp4" {
        if size <= MP4_MIN_BYTES {
            return Ok(None);
        }
        if probe_video_efficient(source_path) {
            return Ok(None);
        }
    }
    if !media_tools::tool_on_path("ffmpeg") {
        bail!("ffmpeg required for video derived media");
    }
    let out = work_dir.join(format!(
        "out-{}.mp4",
        hash_file_prefix(source_path).unwrap_or_else(|| "vid".into())
    ));
    media_tools::run_ffmpeg(
        &[
            "-i",
            path_str(source_path)?,
            "-vf",
            "scale='if(gt(iw,ih),-2,min(720,iw))':'if(gt(iw,ih),min(720,ih),-2)',fps=30",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "28",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-movflags",
            "+faststart",
            path_str(&out)?,
        ],
        Some(&out),
    )?;
    Ok(Some(out))
}

fn derive_audio(source_path: &Path, work_dir: &Path) -> Result<Option<PathBuf>> {
    let ext = ext_of(source_path);
    let size = fs::metadata(source_path)?.len();
    if ext == ".mp3" && size <= MP3_MIN_BYTES {
        return Ok(None);
    }
    if !media_tools::tool_on_path("ffmpeg") {
        bail!("ffmpeg required for audio derived media");
    }
    let out = work_dir.join(format!(
        "out-{}.mp3",
        source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio")
    ));
    media_tools::run_ffmpeg(
        &[
            "-i",
            path_str(source_path)?,
            "-vn",
            "-ac",
            "1",
            "-c:a",
            "libmp3lame",
            "-q:a",
            "6",
            path_str(&out)?,
        ],
        Some(&out),
    )?;
    Ok(Some(out))
}

fn hash_file_prefix(path: &Path) -> Option<String> {
    crate::assets::hash_file(path)
        .ok()
        .map(|h| h[..12].to_string())
}

/// Content-addressed relative path: `<aa>/<sha><ext>`.
pub fn derived_rel_path(sha256: &str, ext: &str) -> String {
    crate::assets::shard_rel_path(sha256, ext)
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        ".jpg" | ".jpeg" => "image/jpeg",
        ".mp4" => "video/mp4",
        ".mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn store_derived_bytes(derived_dir: &Path, buf: &[u8], ext: &str) -> Result<DerivedBlob> {
    let sha = crate::assets::sha256_hex(buf);
    let rel = derived_rel_path(&sha, ext);
    let dest = derived_dir.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if !dest.exists() {
        let mut f = File::create(&dest).with_context(|| format!("create {}", dest.display()))?;
        f.write_all(buf)?;
    }
    Ok(DerivedBlob {
        sha256: sha,
        assets_path: rel,
        mime_type: mime_for_ext(ext).to_string(),
    })
}

fn store_derived_file(derived_dir: &Path, file_path: &Path, ext: &str) -> Result<DerivedBlob> {
    let buf = fs::read(file_path)?;
    store_derived_bytes(derived_dir, &buf, ext)
}

/// Whether an existing derived file should be skipped (idempotency).
fn should_skip_existing(
    force: bool,
    derived_assets_path: Option<&str>,
    converted_dir: &Path,
) -> bool {
    if force {
        return false;
    }
    match derived_assets_path {
        Some(rel) if !rel.is_empty() => converted_dir.join(rel).is_file(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::engine;

    #[test]
    fn derived_rel_path_layout() {
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert_eq!(derived_rel_path(sha, ".jpg"), format!("ab/{sha}.jpg"));
        assert_eq!(derived_rel_path(sha, ".jpeg"), format!("ab/{sha}.jpg"));
    }

    const SHA: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    #[test]
    fn kind_classifies_and_skips_gif() {
        assert_eq!(kind_of("x.jpg", None, &[]), MediaKind::Image);
        assert_eq!(kind_of("x.mp4", None, &[]), MediaKind::Video);
        assert_eq!(kind_of("x.m4a", None, &[]), MediaKind::Audio);
        assert_eq!(kind_of("x.gif", None, &[]), MediaKind::Other);
        assert_eq!(kind_of("x.bin", Some("image/png"), &[]), MediaKind::Image);
    }

    #[test]
    fn extensionless_blobs_classify_from_the_attachment_name() {
        let canonical = format!("ab/{SHA}");
        for (name, expected) in [
            ("voice-note.amr", MediaKind::Audio),
            ("memo.wav", MediaKind::Audio),
            ("podcast.ogg", MediaKind::Audio),
            ("clip.3gp", MediaKind::Video),
            ("clip.webm", MediaKind::Video),
            ("movie.mkv", MediaKind::Video),
            ("scan.tiff", MediaKind::Image),
            ("notes.txt", MediaKind::Other),
        ] {
            assert_eq!(
                kind_of(&canonical, None, &[Some(name), None]),
                expected,
                "unexpected kind for {name}"
            );
            // The original export path is the second-choice hint.
            assert_eq!(
                kind_of(
                    &canonical,
                    Some("  "),
                    &[None, Some(&format!("media/{name}"))]
                ),
                expected,
                "unexpected kind for path hint media/{name}"
            );
        }
    }

    #[test]
    fn attachment_name_hints_never_override_declared_media_types() {
        let canonical = format!("ab/{SHA}");
        // A declared MIME is authoritative, including the deliberate GIF skip.
        assert_eq!(
            kind_of(&canonical, Some("image/gif"), &[Some("clip.mp4")]),
            MediaKind::Other
        );
        assert_eq!(
            kind_of(&canonical, Some("application/pdf"), &[Some("clip.mp4")]),
            MediaKind::Other
        );
        assert_eq!(
            kind_of("ab/photo.gif", None, &[Some("clip.mp4")]),
            MediaKind::Other
        );
        // Incomplete transfers stay out of ffmpeg regardless of the hint.
        assert_eq!(
            kind_of("ab/upload.part", None, &[Some("clip.mp4")]),
            MediaKind::Other
        );
    }

    #[test]
    fn skip_existing_derived_file() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "ab/deadbeef.jpg";
        let dest = dir.path().join(rel);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"x").unwrap();
        assert!(should_skip_existing(false, Some(rel), dir.path()));
        assert!(!should_skip_existing(true, Some(rel), dir.path()));
        assert!(!should_skip_existing(
            false,
            Some("missing.jpg"),
            dir.path()
        ));
        assert!(!should_skip_existing(false, None, dir.path()));
    }

    #[tokio::test]
    async fn store_and_update_derived_db() {
        let (pool, dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO accounts (id, username, read_only) VALUES ('acc', 'demo', 0)")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
             VALUES ('acc', '+1', '+1', 'phone', 'phone')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
             VALUES (1, 'acc', 1, 'individual', 't')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, account_id, source, timestamp, is_from_me, sort_order)
             VALUES (1, 1, 'acc', 'imessage', '2020-01-01T00:00:00Z', 0, 0)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attachments (id, message_id, sha256, assets_path, mime_type)
             VALUES (1, 1, 'aa11', 'aa/aa11.jpg', 'image/jpeg')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let converted = dir.path().join("converted");
        fs::create_dir_all(&converted).unwrap();
        let blob = store_derived_bytes(&converted, b"jpeg-bytes", ".jpg").unwrap();
        assert!(converted.join(&blob.assets_path).is_file());

        update_derived(&mut conn, "acc", "imessage", "aa11", &blob)
            .await
            .unwrap();

        let (d_sha, d_path, d_mime): (String, String, String) = sqlx::query_as(
            "SELECT derived_sha256, derived_assets_path, derived_mime_type FROM attachments WHERE id = 1",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(d_sha, blob.sha256);
        assert_eq!(d_path, blob.assets_path);
        assert_eq!(d_mime, "image/jpeg");
    }

    #[test]
    fn part_paths_are_not_media() {
        assert!(is_part_path("aa/aabbcc.part"));
        assert!(is_part_path("upload.PART"));
        assert!(!is_part_path("aa/aabbcc.mp4"));
        assert_eq!(
            kind_of("aa/x.part", Some("video/mp4"), &[]),
            MediaKind::Other
        );
    }

    #[tokio::test]
    async fn listed_attachments_carry_name_hints_for_extensionless_blobs() {
        let (pool, _dir) = engine::test_pool().await;
        schema::ensure_vault_schema(&mut pool.acquire().await.unwrap())
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(&format!(
            r#"
            INSERT INTO accounts (id, username, read_only) VALUES ('acc', 'demo', 0);
            INSERT INTO handles (account_id, raw, normalized, handle_type, service)
                VALUES ('acc', '+1', '+1', 'phone', 'phone');
            INSERT INTO conversations (id, account_id, chat_handle_id, conversation_type, source_file)
                VALUES (1, 'acc', 1, 'individual', 't');
            INSERT INTO messages (id, conversation_id, account_id, source, timestamp, is_from_me, sort_order)
                VALUES (1, 1, 'acc', 'imessage', '2020-01-01T00:00:00Z', 0, 0);
            INSERT INTO attachments (id, message_id, sha256, assets_path, mime_type, original_name, path)
                VALUES (1, 1, '{SHA}', 'ab/{SHA}', NULL, 'voice-note.amr', 'attachments/voice-note.amr');
            "#
        ))
        .execute(&mut *conn)
        .await
        .unwrap();

        let rows = list_attachments(&mut conn, "acc", "imessage")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            kind_of(
                &row.assets_path,
                row.mime_type.as_deref(),
                &row.name_hints()
            ),
            MediaKind::Audio,
            "an extensionless blob with no declared MIME must classify from its attachment name"
        );
    }

    #[test]
    fn jpeg_under_threshold_skipped_without_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("small.jpg");
        fs::write(&img, vec![0u8; 100]).unwrap();
        // derive_image returns None for small JPEGs before calling ffmpeg.
        let out = derive_image(&img).unwrap();
        assert!(out.is_none());
    }
}
