//! Generator configuration loaded from `demo_seed.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SeedConfig {
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_out")]
    pub out: String,
    pub contacts: ContactsConfig,
    pub labels: LabelsConfig,
    pub one_to_one: OneToOneConfig,
    pub groups: GroupsConfig,
    pub messages: MessagesConfig,
    pub edge_cases: EdgeCasesConfig,
    #[serde(default)]
    pub sources: SourcesConfig,
}

fn default_seed() -> u64 {
    42
}
fn default_out() -> String {
    "demo".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactsConfig {
    pub count: usize,
    pub no_name: f64,
    #[allow(dead_code)] // residual / documentation; sampling uses first_only + first_middle_last
    pub first_last: f64,
    pub first_middle_last: f64,
    pub first_only: f64,
    pub us_phones: f64,
    pub inactive_fraction: f64,
    pub no_messages_fraction: f64,
    pub multi_phone_fraction: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelsConfig {
    #[allow(dead_code)]
    pub names: Vec<String>,
    pub family: f64,
    pub work: f64,
    pub college: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OneToOneConfig {
    pub typical_min: u32,
    pub typical_max: u32,
    pub min_per_year: u32,
    pub max_per_year: u32,
    pub low_tail: f64,
    pub high_tail: f64,
    pub span_mean_years: f64,
    pub span_mean_jitter: f64,
    pub span_max_years: f64,
    pub newest_days: u32,
    pub one_to_one_fraction: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupsConfig {
    pub per_contact_mean: f64,
    pub per_contact_min: u32,
    pub per_contact_max: u32,
    pub participants_mean: f64,
    pub participants_min: u32,
    pub participants_max: u32,
    /// Minimum number of named groups with size in
    /// `[large_participants_min, large_participants_max]`.
    #[serde(default = "default_large_min_count")]
    pub large_min_count: usize,
    #[serde(default = "default_large_participants_min")]
    pub large_participants_min: u32,
    #[serde(default = "default_large_participants_max")]
    pub large_participants_max: u32,
    pub typical_min: u32,
    pub typical_max: u32,
    pub min_per_year: u32,
    pub max_per_year: u32,
    pub low_tail: f64,
    pub high_tail: f64,
    pub span_mean_years: f64,
    pub span_max_years: f64,
    pub phone_only_fraction: f64,
}

fn default_large_min_count() -> usize {
    10
}
fn default_large_participants_min() -> u32 {
    8
}
fn default_large_participants_max() -> u32 {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesConfig {
    pub emoji_probability: f64,
    pub jpg_base_stride: usize,
    pub other_base_stride: usize,
    pub tapback_stride: usize,
    pub reply_stride: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeCasesConfig {
    pub unassigned_phones: usize,
    pub unassigned_emails: usize,
    pub orphaned_messages: usize,
    pub empty_individual: bool,
    pub empty_group: bool,
}

/// How demo conversations are split across backup source trees.
#[derive(Debug, Clone, Deserialize)]
pub struct SourcesConfig {
    /// Fraction of non-overlap 1:1 contacts that are Android-only.
    #[serde(default = "default_android_only_fraction")]
    pub android_only_fraction: f64,
    /// Contacts written into both `imessage` and `sms-backup-restore` staging.
    #[serde(default = "default_overlap_count")]
    pub overlap_count: usize,
    /// Fraction of overlap iMessage messages also present on Android (shared fingerprint).
    #[serde(default = "default_overlap_shared_fraction")]
    pub overlap_shared_fraction: f64,
    #[serde(default = "default_overlap_android_extra_min")]
    pub overlap_android_extra_min: usize,
    #[serde(default = "default_overlap_android_extra_max")]
    pub overlap_android_extra_max: usize,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            android_only_fraction: default_android_only_fraction(),
            overlap_count: default_overlap_count(),
            overlap_shared_fraction: default_overlap_shared_fraction(),
            overlap_android_extra_min: default_overlap_android_extra_min(),
            overlap_android_extra_max: default_overlap_android_extra_max(),
        }
    }
}

fn default_android_only_fraction() -> f64 {
    0.12
}
fn default_overlap_count() -> usize {
    10
}
fn default_overlap_shared_fraction() -> f64 {
    0.35
}
fn default_overlap_android_extra_min() -> usize {
    20
}
fn default_overlap_android_extra_max() -> usize {
    80
}

impl SeedConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read demo-seed config {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        let g = &self.groups;
        if g.large_min_count == 0 {
            return Ok(());
        }
        if g.large_participants_min > g.large_participants_max {
            anyhow::bail!(
                "groups.large_participants_min ({}) > large_participants_max ({})",
                g.large_participants_min,
                g.large_participants_max
            );
        }
        if g.large_participants_min < g.participants_min {
            anyhow::bail!(
                "groups.large_participants_min ({}) < participants_min ({})",
                g.large_participants_min,
                g.participants_min
            );
        }
        if g.large_participants_max > g.participants_max {
            anyhow::bail!(
                "groups.large_participants_max ({}) > participants_max ({})",
                g.large_participants_max,
                g.participants_max
            );
        }
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo_seed.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_large_band_outside_participants_max() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load");
        cfg.groups.large_participants_max = cfg.groups.participants_max + 1;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_inverted_large_band() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load");
        cfg.groups.large_participants_min = 15;
        cfg.groups.large_participants_max = 10;
        assert!(cfg.validate().is_err());
    }
}
