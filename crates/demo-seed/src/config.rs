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
    pub msgs_per_year_min: u32,
    pub msgs_per_year_max: u32,
    pub span_mean_years: f64,
    pub span_max_years: f64,
    pub phone_only_fraction: f64,
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

impl SeedConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read demo-seed config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo_seed.toml")
    }
}
