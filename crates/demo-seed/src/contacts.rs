use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::personas::{OWNER_PHONE, Roster};

pub fn write_vcf(config_dir: &Path, roster: &Roster) -> Result<()> {
    let contacts_path = config_dir.join("contacts.vcf");
    let mut out = String::new();
    for c in &roster.contacts {
        let display_name = [c.first_name.trim(), c.last_name.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(out, "BEGIN:VCARD")?;
        writeln!(out, "VERSION:3.0")?;
        writeln!(out, "FN:{}", escape_vcf(&display_name))?;
        writeln!(
            out,
            "N:{};{};;;",
            escape_vcf(&c.last_name),
            escape_vcf(&c.first_name)
        )?;
        for phone in &c.phones {
            writeln!(out, "TEL:{}", escape_vcf(phone))?;
        }
        if !c.groups.is_empty() {
            writeln!(
                out,
                "CATEGORIES:{}",
                c.groups
                    .iter()
                    .map(|label| escape_vcf(label))
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
        }
        writeln!(out, "END:VCARD")?;
    }
    fs::write(&contacts_path, out).with_context(|| format!("write {}", contacts_path.display()))?;
    Ok(())
}

fn escape_vcf(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

pub fn write_config_toml(config_dir: &Path) -> Result<()> {
    let path = config_dir.join("config.toml");
    let body = r#"# Instance config restored by `reset-demo`.
# Demo account identity lives in demo/config/seed.toml.

[paths]
db = "data/vault.db"
data_dir = "data"
assets_dir = "assets"
assets_converted_dir = "assets_converted"

# Optional HTTP import API for local development.
# [server]
# bind = "127.0.0.1:8080"
"#;
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn write_seed_toml(config_dir: &Path) -> Result<()> {
    let path = config_dir.join("seed.toml");
    let body = format!(
        r#"# Demo account identity used only by `reset-demo`.
# Not copied into the runtime config.toml.

[owner]
display_name = "Demo User"
phones = ["{OWNER_PHONE}"]
emails = ["demo.ingest@example.com"]

[account]
username = "demo"
read_only = true
"#
    );
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
