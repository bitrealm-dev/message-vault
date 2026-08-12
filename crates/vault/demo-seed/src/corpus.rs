//! Public-domain sentence corpus loader.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rand::Rng;
use rand::seq::IndexedRandom;

pub struct Corpus {
    sentences: Vec<String>,
}

impl Corpus {
    pub fn load_pride_and_prejudice() -> Result<Self> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/corpus/pride-and-prejudice.txt");
        Self::load(&path)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("read corpus {}", path.display()))?;
        let sentences = extract_sentences(&text);
        if sentences.len() < 100 {
            bail!(
                "corpus {} yielded only {} sentences (need ≥100)",
                path.display(),
                sentences.len()
            );
        }
        Ok(Self { sentences })
    }

    pub fn pick(&self, rng: &mut impl Rng) -> &str {
        self.sentences
            .choose(rng)
            .map(|s| s.as_str())
            .unwrap_or("Okay.")
    }

    /// One or two sentences joined for slightly longer messages.
    pub fn pick_message(&self, rng: &mut impl Rng) -> String {
        let first = self.pick(rng).to_string();
        if rng.random_bool(0.18) {
            let second = self.pick(rng);
            if second != first {
                return format!("{first} {second}");
            }
        }
        first
    }

    pub fn len(&self) -> usize {
        self.sentences.len()
    }
}

fn extract_sentences(text: &str) -> Vec<String> {
    let mut flat = String::with_capacity(text.len());
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            flat.push(' ');
            continue;
        }
        if t.starts_with('[') || is_heading(t) {
            flat.push(' ');
            continue;
        }
        flat.push_str(t);
        flat.push(' ');
    }

    // Normalize curly quotes / dashes so splitters see plain prose.
    let flat = flat
        .replace(['“', '”', '„'], "\"")
        .replace(['‘', '’'], "'")
        .replace(['—', '–'], "-");

    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in flat.as_bytes().iter().enumerate() {
        let c = b as char;
        if matches!(c, '.' | '!' | '?') {
            let end = i + 1;
            if let Some(s) = normalize_sentence(&flat[start..end]) {
                out.push(s);
            }
            start = end;
        }
    }
    out
}

fn is_heading(t: &str) -> bool {
    let u = t.to_ascii_uppercase();
    u.starts_with("CHAPTER ")
        || u.starts_with("VOLUME ")
        || u.starts_with("CHAPTER ")
        || u == "PRIDE AND PREJUDICE"
}

fn normalize_sentence(raw: &str) -> Option<String> {
    let s = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '*' | '_' | '-' | '[' | ']'))
        .to_string();
    if s.len() < 20 || s.len() > 180 {
        return None;
    }
    if !s.chars().any(|c| c.is_ascii_lowercase()) {
        return None;
    }
    // Skip footnote-ish leftovers.
    if s.chars().filter(|c| c.is_ascii_digit()).count() > s.len() / 3 {
        return None;
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bundled_corpus() {
        let c = Corpus::load_pride_and_prejudice().unwrap();
        assert!(c.len() >= 1000, "got {}", c.len());
    }
}
