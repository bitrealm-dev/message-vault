use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use sha2::{Digest, Sha256};

/// Demo JPEG photos (path relative to export dir, display name).
pub struct JpgPhoto {
    pub path: &'static str,
    pub original_name: &'static str,
}

pub const JPG_PHOTOS: &[JpgPhoto] = &[
    JpgPhoto {
        path: "attachments/sunset.jpg",
        original_name: "IMG_2847.jpg",
    },
    JpgPhoto {
        path: "attachments/park.jpg",
        original_name: "IMG_3102.jpg",
    },
    JpgPhoto {
        path: "attachments/dinner.jpg",
        original_name: "IMG_4521.jpg",
    },
    JpgPhoto {
        path: "attachments/puppy.jpg",
        original_name: "IMG_5098.jpg",
    },
    JpgPhoto {
        path: "attachments/receipt.jpg",
        original_name: "Scan_2024-03-15.jpg",
    },
    JpgPhoto {
        path: "attachments/selfie.jpg",
        original_name: "IMG_6110.jpg",
    },
    JpgPhoto {
        path: "attachments/beach.jpg",
        original_name: "IMG_7203.jpg",
    },
    JpgPhoto {
        path: "attachments/flowers.jpg",
        original_name: "IMG_8011.jpg",
    },
];

/// Non-JPEG attachments so the demo includes more than photos.
pub const OTHER_ATTACHMENTS: &[(&str, &str, bool)] = &[
    ("attachments/landscape.png", "image/png", false),
    ("attachments/sticker.gif", "image/gif", true),
    ("attachments/voice.caf", "audio/x-caf", false),
    ("attachments/notes.pdf", "application/pdf", false),
    ("attachments/missing-file.heic", "image/heic", false),
];

/// Write colorful JPEGs large enough to show inline in the web UI.
///
/// Returns a map from attachment path to a short fingerprint of the file
/// contents and the file size in bytes. Conversation files store those
/// values next to each attachment.
pub fn write_attachment_blobs(dir: &Path) -> Result<HashMap<String, (String, u64)>> {
    let specs: &[(&str, [u8; 3])] = &[
        ("sunset.jpg", [255, 140, 60]),
        ("park.jpg", [72, 160, 95]),
        ("dinner.jpg", [180, 85, 70]),
        ("puppy.jpg", [210, 175, 130]),
        ("receipt.jpg", [245, 245, 240]),
        ("selfie.jpg", [90, 130, 200]),
        ("beach.jpg", [60, 175, 220]),
        ("flowers.jpg", [220, 100, 150]),
    ];
    let mut digests = HashMap::new();
    for (name, rgb) in specs {
        let path = dir.join(name);
        write_color_jpeg(&path, *rgb, 320, 240)?;
        let bytes = std::fs::read(&path)?;
        record_blob(&mut digests, &format!("attachments/{name}"), &bytes);
    }

    fs::write(dir.join("landscape.png"), MINI_PNG)?;
    record_blob(&mut digests, "attachments/landscape.png", MINI_PNG);

    fs::write(dir.join("sticker.gif"), MINI_GIF)?;
    record_blob(&mut digests, "attachments/sticker.gif", MINI_GIF);

    fs::write(dir.join("voice.caf"), MINI_CAF)?;
    record_blob(&mut digests, "attachments/voice.caf", MINI_CAF);

    fs::write(dir.join("notes.pdf"), MINI_PDF)?;
    record_blob(&mut digests, "attachments/notes.pdf", MINI_PDF);

    // attachments/missing-file.heic is left out of this map on purpose.
    // Conversation JSONL still points at it, but the file is not on disk, so
    // import can show its missing-file warning.

    Ok(digests)
}

fn record_blob(digests: &mut HashMap<String, (String, u64)>, relative_path: &str, bytes: &[u8]) {
    let sha = hex::encode(Sha256::digest(bytes));
    digests.insert(relative_path.into(), (sha, bytes.len() as u64));
}

fn write_color_jpeg(path: &Path, rgb: [u8; 3], width: u32, height: u32) -> Result<()> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        // Subtle gradient so thumbnails are visibly distinct.
        let r = rgb[0].saturating_add(((x * 40) / width) as u8);
        let g = rgb[1].saturating_add(((y * 30) / height) as u8);
        let b = rgb[2];
        Rgb([r, g, b])
    });
    img.save(path)
        .with_context(|| format!("write jpeg {}", path.display()))?;
    Ok(())
}

// Tiny valid 1x1 red PNG.
const MINI_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

const MINI_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
];

const MINI_CAF: &[u8] = b"caff\x00\x00\x00\x1C\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

const MINI_PDF: &[u8] = b"%PDF-1.1\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n";
