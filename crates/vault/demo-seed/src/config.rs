//! Loads generator settings from `demo_seed.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, de};

/// Settings loaded from `demo_seed.toml`: how many contacts, how conversations
/// are split across backups, and how often messages get photos or replies.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedConfig {
    /// Random seed. The same seed and settings produce the same backups.
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Folder the generated backups are written under.
    #[serde(default = "default_out")]
    pub out: String,
    /// The "now" that every generated timestamp counts back from.
    #[serde(deserialize_with = "deserialize_reference_time")]
    pub reference_time: DateTime<Utc>,
    /// How many contacts to invent and how their handles are shaped.
    pub contacts: ContactsConfig,
    /// Contact labels and the share of contacts that get each one.
    pub labels: LabelsConfig,
    /// One-to-one conversation sizes and shapes.
    pub one_to_one: OneToOneConfig,
    /// Group conversation counts and sizes.
    pub groups: GroupsConfig,
    /// Message mix: attachments, replies, tapbacks, transports.
    pub messages: MessagesConfig,
    /// Deliberately awkward data: unassigned handles, orphans, empty threads.
    pub edge_cases: EdgeCasesConfig,
    /// How conversations are split across the backup folders.
    #[serde(default)]
    pub sources: SourcesConfig,
}

/// serde default for `seed`.
fn default_seed() -> u64 {
    42
}
/// serde default for `out`.
fn default_out() -> String {
    "crates/vault/demo-seed".into()
}

/// Read a timestamp string such as `2026-08-01T12:00:00Z` and convert it to UTC.
fn deserialize_reference_time<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    DateTime::parse_from_rfc3339(&value)
        .map(|date_time| date_time.with_timezone(&Utc))
        .map_err(de::Error::custom)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactsConfig {
    pub count: usize,
    pub no_name: f64,
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
    /// At least this many groups must have a participant count between
    /// `large_participants_min` and `large_participants_max`.
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

/// serde default for `large_min_count`.
fn default_large_min_count() -> usize {
    10
}
/// serde default for `large_participants_min`.
fn default_large_participants_min() -> u32 {
    8
}
/// serde default for `large_participants_max`.
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
    /// Share of messages in the iMessage folder that are marked as SMS or RCS
    /// so the conversation view can show those labels.
    #[serde(default = "default_apple_fallback_transport_fraction")]
    pub apple_fallback_transport_fraction: f64,
}

/// serde default for `apple_fallback_transport_fraction`.
fn default_apple_fallback_transport_fraction() -> f64 {
    0.20
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeCasesConfig {
    pub unassigned_phones: usize,
    pub unassigned_emails: usize,
    pub orphaned_messages: usize,
    pub empty_individual: bool,
    pub empty_group: bool,
}

/// How demo conversations are split across the iMessage, Android, and WhatsApp folders.
#[derive(Debug, Clone, Deserialize)]
pub struct SourcesConfig {
    /// Share of one-to-one contacts (excluding the ones that appear in both
    /// backups) that only appear in the Android backup.
    #[serde(default = "default_android_only_fraction")]
    pub android_only_fraction: f64,
    /// How many contacts are written into both the iMessage and Android folders.
    #[serde(default = "default_overlap_count")]
    pub overlap_count: usize,
    /// Share of messages in those overlapping iMessage threads that also appear
    /// in the Android backup with the same text and time.
    #[serde(default = "default_overlap_shared_fraction")]
    pub overlap_shared_fraction: f64,
    #[serde(default = "default_overlap_android_extra_min")]
    pub overlap_android_extra_min: usize,
    #[serde(default = "default_overlap_android_extra_max")]
    pub overlap_android_extra_max: usize,
    /// Share of contacts that also get a WhatsApp conversation. That conversation
    /// uses the same phone number, marked as WhatsApp rather than iMessage or SMS.
    #[serde(default = "default_whatsapp_contact_fraction")]
    pub whatsapp_contact_fraction: f64,
}

impl Default for SourcesConfig {
    /// Values used when `demo_seed.toml` omits the `[sources]` section.
    fn default() -> Self {
        Self {
            android_only_fraction: default_android_only_fraction(),
            overlap_count: default_overlap_count(),
            overlap_shared_fraction: default_overlap_shared_fraction(),
            overlap_android_extra_min: default_overlap_android_extra_min(),
            overlap_android_extra_max: default_overlap_android_extra_max(),
            whatsapp_contact_fraction: default_whatsapp_contact_fraction(),
        }
    }
}

/// serde default for `android_only_fraction`.
fn default_android_only_fraction() -> f64 {
    0.12
}
/// serde default for `overlap_count`.
fn default_overlap_count() -> usize {
    10
}
/// serde default for `overlap_shared_fraction`.
fn default_overlap_shared_fraction() -> f64 {
    0.35
}
/// serde default for `overlap_android_extra_min`.
fn default_overlap_android_extra_min() -> usize {
    20
}
/// serde default for `overlap_android_extra_max`.
fn default_overlap_android_extra_max() -> usize {
    80
}
/// serde default for `whatsapp_contact_fraction`.
fn default_whatsapp_contact_fraction() -> f64 {
    0.20
}

impl SeedConfig {
    /// Load settings from a TOML file and check that group-size ranges make sense.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the TOML is invalid, or
    /// [`Self::validate`] fails.
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read demo-seed config {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Check that the large-group size range sits inside the overall group size range.
    ///
    /// # Errors
    ///
    /// Returns an error if the minimum is larger than the maximum, or if the
    /// large-group range sticks out past the overall min or max.
    pub fn validate(&self) -> Result<()> {
        if self.labels.names.len() != 4 {
            anyhow::bail!(
                "labels.names must have exactly 4 entries (family, work, college, inactive), found {}",
                self.labels.names.len()
            );
        }
        if self.contacts.first_only + self.contacts.first_middle_last + self.contacts.first_last
            > 1.0
        {
            anyhow::bail!(
                "contacts name-shape shares must sum to at most 1.0 (first_only {} + first_middle_last {} + first_last {} = {})",
                self.contacts.first_only,
                self.contacts.first_middle_last,
                self.contacts.first_last,
                self.contacts.first_only
                    + self.contacts.first_middle_last
                    + self.contacts.first_last
            );
        }
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

    /// Path to `demo_seed.toml` next to this crate's `Cargo.toml`.
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

    #[test]
    fn rejects_labels_names_without_four_entries() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load");
        cfg.labels.names = vec!["Family".into(), "Work".into()];
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_name_shape_shares_above_one() {
        let mut cfg = SeedConfig::load(&SeedConfig::default_path()).expect("load");
        cfg.contacts.first_last = 1.0;
        assert!(cfg.validate().is_err());
    }
}
