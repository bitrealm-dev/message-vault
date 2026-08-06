//! Name list loader and sampling.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rand::Rng;
use rand::seq::IndexedRandom;

pub struct NameBank {
    pub first: Vec<String>,
    pub last: Vec<String>,
    pub middle: Vec<String>,
}

impl NameBank {
    pub fn load_default() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/names");
        Ok(Self {
            first: load_lines(&root.join("first_names.txt"))?,
            last: load_lines(&root.join("last_names.txt"))?,
            middle: load_lines(&root.join("middle_names.txt"))?,
        })
    }

    pub fn pick_first(&self, rng: &mut impl Rng) -> &str {
        self.first.choose(rng).map(|s| s.as_str()).unwrap_or("Alex")
    }

    pub fn pick_last(&self, rng: &mut impl Rng) -> &str {
        self.last.choose(rng).map(|s| s.as_str()).unwrap_or("Lee")
    }

    pub fn pick_middle(&self, rng: &mut impl Rng) -> &str {
        self.middle
            .choose(rng)
            .map(|s| s.as_str())
            .unwrap_or("Lee")
    }
}

fn load_lines(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        bail!("{} is empty", path.display());
    }
    Ok(lines)
}
