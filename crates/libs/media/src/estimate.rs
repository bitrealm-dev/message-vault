//! Forecast what the media step will do to a staged file's size.
//!
//! Every number here is an estimate and the screen says so. The point is not
//! precision: it is telling the difference between a file that will comfortably
//! fit, one that will not, and one that is fine now and will not be afterwards.

use std::path::Path;

use crate::process::{Kind, classify, is_efficient};
use crate::{CompressOptions, MediaMode, MediaProbe};

/// Files smaller than this fraction of the limit are not probed.
///
/// The largest growth factor in [`format_factor`] is well under 2.5x, so a file
/// this far below the limit cannot cross it, and probing every thumbnail in a
/// backup costs more than the answer is worth (decision 13).
const PROBE_BAND_FLOOR: f64 = 0.4;

/// An over-limit file whose estimate lands above this fraction of the limit
/// reads as probably still too big rather than likely to fit.
///
/// The margin is what stops a near miss from reading as a promise (decision 13).
pub const PROBABLY_FITS_MARGIN: f64 = 0.8;

/// How a staged attachment is expected to land against the size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeVerdict {
    /// Under the limit now, and expected to stay under.
    FitsAsIs,
    /// Over the limit now, expected to come under after the media step.
    LikelyFits,
    /// Under the limit now, expected to cross it during the media step.
    MayGrow,
    /// Over the limit now, and expected to stay over.
    ProbablyTooBig,
    /// The media step does not handle this kind of file, so its size is fixed.
    CannotProcess,
}

/// Is this file worth an ffprobe call?
#[must_use]
pub fn needs_probe(size_bytes: u64, limit_bytes: u64) -> bool {
    size_bytes as f64 >= limit_bytes as f64 * PROBE_BAND_FLOOR
}

/// Estimated size after the media step, in bytes. Never capped at the original.
///
/// `ext` is matched case-insensitively (normalized internally), so callers
/// may pass it exactly as read from a file name.
#[must_use]
pub fn estimate_bytes(
    size_bytes: u64,
    probe: Option<&MediaProbe>,
    ext: &str,
    mode: MediaMode,
    compress: &CompressOptions,
) -> u64 {
    let ext = ext.to_ascii_lowercase();
    let ext = ext.as_str();
    if skipped_as_efficient(ext, probe, mode, compress) {
        return size_bytes;
    }
    let factor = format_factor(ext, probe, mode);
    let scale = match (probe, mode) {
        // Only compress scales video. convert_video re-encodes at the source
        // resolution, so its size change is entirely the format's doing.
        (Some(p), MediaMode::Compress) if p.fps.is_some() => {
            pixel_ratio(p, compress) * fps_ratio(p, compress)
        }
        _ => 1.0,
    };
    (size_bytes as f64 * scale * factor).round() as u64
}

/// Classify one file, probing it first when it is close enough to matter.
///
/// `ext` is matched case-insensitively (normalized internally), so callers
/// may pass it exactly as read from a file name.
#[must_use]
pub fn classify_probed(
    size_bytes: u64,
    probe: Option<&MediaProbe>,
    ext: &str,
    mode: MediaMode,
    compress: &CompressOptions,
    limit_bytes: u64,
) -> SizeVerdict {
    let ext = ext.to_ascii_lowercase();
    let ext = ext.as_str();
    if !processable(ext) {
        return size_only(size_bytes, limit_bytes, SizeVerdict::CannotProcess);
    }
    if untouched_by(ext, mode) || skipped_as_efficient(ext, probe, mode, compress) {
        return size_only(size_bytes, limit_bytes, SizeVerdict::ProbablyTooBig);
    }
    if !needs_probe(size_bytes, limit_bytes) {
        return SizeVerdict::FitsAsIs;
    }
    let estimate = estimate_bytes(size_bytes, probe, ext, mode, compress);
    if size_bytes <= limit_bytes {
        return if estimate > limit_bytes {
            SizeVerdict::MayGrow
        } else {
            SizeVerdict::FitsAsIs
        };
    }
    if (estimate as f64) <= limit_bytes as f64 * PROBABLY_FITS_MARGIN {
        SizeVerdict::LikelyFits
    } else {
        SizeVerdict::ProbablyTooBig
    }
}

/// A file whose size the media step will not change is judged on that size.
fn size_only(size_bytes: u64, limit_bytes: u64, over: SizeVerdict) -> SizeVerdict {
    if size_bytes <= limit_bytes {
        SizeVerdict::FitsAsIs
    } else {
        over
    }
}

/// Does the media pass recognize this extension at all?
///
/// Mirrors [`crate::process::classify`] exactly — same three extension lists,
/// `false` for anything else — by calling it on a synthetic path, so a new
/// extension added to the media pass cannot be missed by the forecast.
fn processable(ext: &str) -> bool {
    classify(&Path::new("f").with_extension(ext)).is_some()
}

/// Does the media step leave a file with this extension alone in this mode,
/// independent of size?
///
/// Mirrors the early returns in `process_one`/`run_one` that do not depend on
/// a size gate: nothing is touched in `Clone`/`Disabled` (`run_one`'s last
/// arm skips every file), GIF is untouched in either of the remaining modes,
/// JPEG is already in `Convert`'s target form, MP3 already in `Convert`'s.
/// Deliberately hand-written rather than delegated to
/// [`crate::derivative_name`]: that function decides the JPEG/MP3 `Compress`
/// case by statting the file on disk, and this classifier is handed a size it
/// already knows for a file that may not exist on disk at all (a forecast
/// runs before any conversion). Reusing it here would mean synthesizing a
/// path to stat — and a missing path reads as size zero, which would wrongly
/// mark every JPEG/MP3 in `Compress` mode as untouched regardless of its real
/// size. The 500 KB / 100 KB floors stay out of this function on purpose: a
/// file under either floor is small enough that it never reaches this check
/// (`classify_probed` already answered `FitsAsIs` before probing), so leaving
/// them out cannot make this function disagree with the pass.
fn untouched_by(ext: &str, mode: MediaMode) -> bool {
    if matches!(mode, MediaMode::Clone | MediaMode::Disabled) {
        return true;
    }
    matches!(
        (ext, mode),
        ("gif", _) | ("jpg" | "jpeg", MediaMode::Convert) | ("mp3", MediaMode::Convert)
    )
}

/// Would `compress_video` skip re-encoding this file and only remux it,
/// because it is already an efficient HEVC stream?
///
/// Calls the pass's own [`is_efficient`] predicate rather than restating its
/// codec/resolution/bitrate thresholds, so the forecast cannot drift from
/// what `compress_video` (process.rs) actually decides. Only applies to
/// video in `Compress` mode with `skip_efficient` on and an actual probe in
/// hand — an un-probed file (outside the probe band, or an audio/image file
/// this crate never calls ffprobe for) cannot be judged efficient, so this
/// answers `false` rather than guessing.
fn skipped_as_efficient(
    ext: &str,
    probe: Option<&MediaProbe>,
    mode: MediaMode,
    compress: &CompressOptions,
) -> bool {
    if !matches!(mode, MediaMode::Compress) || !compress.skip_efficient {
        return false;
    }
    if !matches!(
        classify(&Path::new("f").with_extension(ext)),
        Some(Kind::Video)
    ) {
        return false;
    }
    let Some(probe) = probe else {
        return false;
    };
    is_efficient(
        &probe.codec,
        probe.width,
        probe.height,
        probe.bitrate,
        compress,
    )
}

fn pixel_ratio(probe: &MediaProbe, compress: &CompressOptions) -> f64 {
    let source_long = f64::from(probe.width.max(probe.height));
    if source_long <= 0.0 {
        return 1.0;
    }
    let target_long = f64::from(compress.max_resolution.max_long_edge());
    let ratio = (target_long / source_long).min(1.0);
    ratio * ratio
}

fn fps_ratio(probe: &MediaProbe, compress: &CompressOptions) -> f64 {
    let Some(source) = probe.fps.filter(|f| *f > 0.0) else {
        return 1.0;
    };
    let target = if compress.max_fps > 0.0 {
        compress.max_fps
    } else {
        30.0
    };
    f64::from(target / source).min(1.0)
}

/// Size change from the format alone, holding pixels and frame rate fixed.
///
/// Above 1.0 means the target format is bulkier than the source — the case
/// decision 12 exists to catch, and the common one on an iPhone backup.
fn format_factor(ext: &str, probe: Option<&MediaProbe>, mode: MediaMode) -> f64 {
    let compressing = matches!(mode, MediaMode::Compress);
    match ext {
        // Apple stills. HEIC is roughly half an equivalent JPEG, so it grows.
        "heic" | "heif" => 1.8,
        // Lossless and near-lossless stills re-encoded to JPEG.
        "png" | "tif" | "tiff" | "bmp" => 1.3,
        "webp" => 1.2,
        // Already JPEG: only compress touches it, at -q:v 5.
        "jpg" | "jpeg" => 0.7,
        // Already MP3: only compress touches it, at 96k mono.
        "mp3" => 0.6,
        // Anything else to MP3.
        "m4a" | "aac" | "caf" | "amr" | "wav" | "ogg" | "opus" => 0.8,
        // Video: the codec decides, not the container.
        //
        // `convert_video` always lands on H.264 (its remux path preserves the
        // source codec and so is not size-changing at all; only its re-encode
        // fallback is format_factor's concern), so a source already on a more
        // efficient codec grows — decision 12's headline case.
        //
        // `compress_video` re-encodes to HEVC (libx265) at a fixed CRF, so an
        // already-efficient HEVC source never reaches this arm at all — it is
        // caught upstream by `skipped_as_efficient` and judged on its
        // unchanged size instead. What *does* land here in `Compress` mode is
        // a codec (HEVC included) that failed the efficiency check — too big,
        // too high-bitrate, or the wrong resolution — so the flat 0.7 general
        // compress factor applies uniformly; there is no case left where the
        // convert-mode growth factor also belongs to a compressing file.
        _ => match probe.map(|p| p.codec.as_str()) {
            Some("hevc" | "vp9" | "av1") if !compressing => 1.4,
            Some(_) if compressing => 0.7,
            _ => 1.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: u64 = 50 * 1024 * 1024;

    fn probe(codec: &str, width: u32, height: u32, fps: Option<f32>, bitrate: u64) -> MediaProbe {
        MediaProbe {
            codec: codec.into(),
            width,
            height,
            fps,
            bitrate,
        }
    }

    #[test]
    fn a_small_file_is_fine_without_probing() {
        // Under the probe band: no ffprobe call, decided on size alone.
        assert_eq!(
            classify_probed(
                1024,
                None,
                "heic",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::FitsAsIs
        );
    }

    #[test]
    fn heic_under_the_limit_may_grow_past_it() {
        // Decision 12's headline case: HEIC is about half an equivalent JPEG,
        // so converting grows it. 30 MB in, over 50 MB out.
        let p = probe("hevc", 4032, 3024, None, 0);
        assert_eq!(
            classify_probed(
                30 * 1024 * 1024,
                Some(&p),
                "heic",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::MayGrow
        );
    }

    #[test]
    fn uppercase_extension_is_matched_case_insensitively() {
        // Same fixture and expectation as the test above, spelled the way a
        // real file name would be: `IMG_0001.HEIC`. `format_factor` matches
        // extensions as lowercase literals, so an ext this function does not
        // normalize first would miss the "heic" arm, fall through to the
        // video codec arm instead, and read as `FitsAsIs`.
        let p = probe("hevc", 4032, 3024, None, 0);
        assert_eq!(
            classify_probed(
                30 * 1024 * 1024,
                Some(&p),
                "HEIC",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::MayGrow
        );
    }

    #[test]
    fn a_huge_video_compressed_down_is_likely_to_fit() {
        // 4K60 at 400 MB, compressed to 1080p30: pixel ratio 0.25, fps ratio
        // 0.5, format factor 0.7 (HEVC over the efficient-skip's resolution
        // cap, so it actually gets re-encoded) — 400 * 0.25 * 0.5 * 0.7 =
        // 35 MB, comfortably under the 40 MB margin. The bitrate is set high
        // so the efficient-skip gate (which the resolution already fails)
        // isn't what's carrying this test.
        let p = probe("hevc", 3840, 2160, Some(60.0), 20_000_000);
        assert_eq!(
            classify_probed(
                400 * 1024 * 1024,
                Some(&p),
                "mov",
                MediaMode::Compress,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::LikelyFits
        );
    }

    #[test]
    fn a_video_that_stays_over_the_limit_says_so() {
        let p = probe("h264", 1920, 1080, Some(30.0), 0);
        assert_eq!(
            classify_probed(
                900 * 1024 * 1024,
                Some(&p),
                "mp4",
                MediaMode::Compress,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn an_estimate_just_under_the_limit_still_reads_as_too_big() {
        // The 80% margin: a near miss must not read as a promise.
        let p = probe("h264", 1920, 1080, Some(30.0), 0);
        let size = 60 * 1024 * 1024;
        let estimate = estimate_bytes(
            size,
            Some(&p),
            "mp4",
            MediaMode::Compress,
            &CompressOptions::default(),
        );
        assert!(estimate < LIMIT, "test needs an estimate under the limit");
        assert!(estimate > (LIMIT as f64 * PROBABLY_FITS_MARGIN) as u64);
        assert_eq!(
            classify_probed(
                size,
                Some(&p),
                "mp4",
                MediaMode::Compress,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn efficient_hevc_is_skipped_by_compress_so_it_stays_too_big() {
        // Decision the review caught: `compress_video` skips re-encoding
        // (and only remuxes) an HEVC source that is already within the
        // resolution cap and under the ~12 Mbps efficient-bitrate threshold
        // (`is_efficient` in process.rs). A forecast that does not know this
        // scales the file down as if it were being re-encoded — 55 MB * 0.7
        // = 38.5 MB, under the 40 MB margin — and promises `LikelyFits` for
        // a file the pass will not actually touch. It stays 55 MB, over the
        // 50 MB limit: `ProbablyTooBig`.
        let p = probe("hevc", 1920, 1080, Some(30.0), 9_000_000);
        assert_eq!(
            classify_probed(
                55 * 1024 * 1024,
                Some(&p),
                "mp4",
                MediaMode::Compress,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn a_file_the_media_step_cannot_touch_says_so() {
        assert_eq!(
            classify_probed(
                80 * 1024 * 1024,
                None,
                "pdf",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::CannotProcess
        );
    }

    #[test]
    fn gif_is_never_processed_so_it_is_judged_on_its_own_size() {
        // process_one skips GIF in both modes. Its size will not change.
        assert_eq!(
            classify_probed(
                80 * 1024 * 1024,
                None,
                "gif",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
        assert_eq!(
            classify_probed(
                1024,
                None,
                "gif",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::FitsAsIs
        );
    }

    #[test]
    fn gif_in_compress_mode_is_also_judged_on_its_own_size() {
        // Same guarantee as the Convert-mode test above, but for Compress —
        // a regression that narrowed `untouched_by`'s GIF arm to Convert
        // only would still pass every other test here (GIF's fallback
        // `format_factor` also happens to be a no-op for an un-probed file),
        // so this uses a probed GIF (ffprobe does report a stream for an
        // animated GIF) specifically to make the wrong branch compute a
        // different number: 55 MB * format_factor 0.7 = 38.5 MB, under the
        // margin, reads `LikelyFits` instead of the correct `ProbablyTooBig`.
        let p = probe("gif", 800, 600, Some(15.0), 0);
        assert_eq!(
            classify_probed(
                55 * 1024 * 1024,
                Some(&p),
                "gif",
                MediaMode::Compress,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn jpeg_in_convert_is_untouched_so_a_big_one_stays_too_big() {
        // Convert mode leaves an already-JPEG file alone (`run_one`'s early
        // return). Dropping that arm from `untouched_by` would instead run
        // it through `format_factor`'s 0.7 "already JPEG" shrink and read
        // `LikelyFits` for a file whose size never actually changes.
        assert_eq!(
            classify_probed(
                55 * 1024 * 1024,
                None,
                "jpg",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn mp3_in_convert_is_untouched_so_a_big_one_stays_too_big() {
        // Same shape as the JPEG case above, for MP3 (`format_factor`'s 0.6
        // "already MP3" shrink is a Compress-mode fact, not a Convert one).
        assert_eq!(
            classify_probed(
                60 * 1024 * 1024,
                None,
                "mp3",
                MediaMode::Convert,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::ProbablyTooBig
        );
    }

    #[test]
    fn clone_and_disabled_leave_every_file_alone() {
        // `run_one`'s last arm skips every file in Clone/Disabled mode
        // regardless of kind or extension. A forecast that does not know
        // this still applies HEIC's 1.8 convert-growth factor and predicts
        // `MayGrow` for a file that will be copied byte-for-byte.
        let p = probe("hevc", 4032, 3024, None, 0);
        assert_eq!(
            classify_probed(
                30 * 1024 * 1024,
                Some(&p),
                "heic",
                MediaMode::Clone,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::FitsAsIs
        );
        assert_eq!(
            classify_probed(
                30 * 1024 * 1024,
                None,
                "heic",
                MediaMode::Disabled,
                &CompressOptions::default(),
                LIMIT
            ),
            SizeVerdict::FitsAsIs
        );
    }

    #[test]
    fn the_estimate_is_not_capped_at_the_original_size() {
        // Decision 12 says so in as many words. A cap would erase MayGrow.
        let p = probe("hevc", 4032, 3024, None, 0);
        let size = 10 * 1024 * 1024;
        assert!(
            estimate_bytes(
                size,
                Some(&p),
                "heic",
                MediaMode::Convert,
                &CompressOptions::default()
            ) > size
        );
    }

    #[test]
    fn a_file_in_the_band_is_worth_probing_and_a_small_one_is_not() {
        assert!(!needs_probe(1024, LIMIT));
        assert!(needs_probe(30 * 1024 * 1024, LIMIT));
        assert!(needs_probe(900 * 1024 * 1024, LIMIT));
    }
}
