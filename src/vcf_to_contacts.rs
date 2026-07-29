use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::phone;
use crate::vcf::{self, VcfCard};

#[derive(Debug, Default)]
pub struct ConvertStats {
    pub cards_total: u64,
    pub cards_skipped_no_tel: u64,
    pub contacts_written: u64,
    pub exclude_only: u64,
}

#[derive(Debug, Clone)]
struct ContactOut {
    phones: Vec<String>,
    first_name: String,
    last_name: String,
    exclude: bool,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExcludeCsvRow {
    phones: String,
    #[serde(default)]
    label: String,
}

pub fn convert(
    vcf_path: &Path,
    out_path: &Path,
    exclude_path: Option<&Path>,
    force: bool,
) -> Result<ConvertStats> {
    if out_path.exists() && !force {
        bail!(
            "refusing to overwrite {} (pass --force)",
            out_path.display()
        );
    }

    let cards = vcf::parse_vcf(vcf_path)?;
    let exclude_map = load_exclude(exclude_path)?;

    let mut stats = ConvertStats {
        cards_total: cards.len() as u64,
        ..Default::default()
    };

    let mut contacts: Vec<ContactOut> = Vec::new();
    let mut phone_to_index: HashMap<String, usize> = HashMap::new();
    let mut merge_warnings: u64 = 0;

    for card in &cards {
        if card.phones.is_empty() {
            stats.cards_skipped_no_tel += 1;
            continue;
        }

        let mut contact = card_to_contact(card);
        apply_exclude(&mut contact, &exclude_map);
        finalize_contact(&mut contact);
        if contact.phones.is_empty() {
            stats.cards_skipped_no_tel += 1;
            continue;
        }

        // Merge into an existing contact if any phone already seen.
        let existing = contact
            .phones
            .iter()
            .find_map(|p| phone_to_index.get(p).copied());

        if let Some(idx) = existing {
            merge_warnings += 1;
            merge_contact(&mut contacts[idx], contact);
            for phone in &contacts[idx].phones {
                phone_to_index.insert(phone.clone(), idx);
            }
            continue;
        }

        let idx = contacts.len();
        for phone in &contact.phones {
            phone_to_index.insert(phone.clone(), idx);
        }
        contacts.push(contact);
    }

    // Exclude-only numbers with no VCF card
    for (number, label) in &exclude_map {
        if phone_to_index.contains_key(number) {
            continue;
        }
        phone_to_index.insert(number.clone(), contacts.len());
        contacts.push(ContactOut {
            phones: vec![number.clone()],
            first_name: label.clone(),
            last_name: String::new(),
            exclude: true,
            tags: Vec::new(),
        });
        stats.exclude_only += 1;
    }

    write_csv(out_path, &contacts)?;
    stats.contacts_written = contacts.len() as u64;
    if merge_warnings > 0 {
        eprintln!(
            "warning: merged {merge_warnings} VCF card(s) that shared phone numbers with earlier cards"
        );
    }
    Ok(stats)
}

fn merge_contact(into: &mut ContactOut, from: ContactOut) {
    for phone in from.phones {
        if !into.phones.iter().any(|p| p == &phone) {
            into.phones.push(phone);
        }
    }
    if into.first_name.is_empty() {
        into.first_name = from.first_name;
    }
    if into.last_name.is_empty() {
        into.last_name = from.last_name;
    }
    into.exclude = into.exclude || from.exclude;
    for tag in from.tags {
        push_tag(&mut into.tags, &tag);
    }
    into.tags.sort();
    into.tags.dedup();
}

fn card_to_contact(card: &VcfCard) -> ContactOut {
    let (fn_stripped, fn_tags) = vcf::extract_tags(&card.fn_raw);
    let first = vcf::strip_tags(&card.n_given);
    let last = vcf::strip_tags(&card.n_family);

    // Mom-style: FN is a single token and last name empty after tag strip
    let nickname = if last.is_empty()
        && !fn_stripped.is_empty()
        && !fn_stripped.contains(' ')
        && (first.is_empty() || first == fn_stripped)
    {
        fn_stripped.clone()
    } else {
        String::new()
    };

    let (first_name, last_name) = if !nickname.is_empty() {
        (nickname, String::new())
    } else {
        (first, last)
    };

    let mut tags = Vec::new();
    for tag in fn_tags {
        let normalized = normalize_tag(&tag);
        if normalized.eq_ignore_ascii_case("People") {
            continue;
        }
        push_tag(&mut tags, &normalized);
    }

    ContactOut {
        phones: card.phones.clone(),
        first_name,
        last_name,
        exclude: false,
        tags,
    }
}

fn apply_exclude(contact: &mut ContactOut, exclude_map: &HashMap<String, String>) {
    if contact.phones.iter().any(|p| exclude_map.contains_key(p)) {
        contact.exclude = true;
    }
}

fn finalize_contact(contact: &mut ContactOut) {
    let mut normalized = Vec::new();
    for raw in std::mem::take(&mut contact.phones) {
        let Some(e164) = phone::to_e164(&raw) else {
            continue;
        };
        if !normalized.iter().any(|p| p == &e164) {
            normalized.push(e164);
        }
    }
    contact.phones = normalized;
    contact.tags.sort();
    contact.tags.dedup();
}

fn normalize_tag(raw: &str) -> String {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("girls") {
        "Girls".to_string()
    } else {
        t.to_string()
    }
}

fn push_tag(tags: &mut Vec<String>, name: &str) {
    if name.is_empty() {
        return;
    }
    if !tags.iter().any(|g| g.eq_ignore_ascii_case(name)) {
        tags.push(name.to_string());
    }
}

fn load_exclude(path: Option<&Path>) -> Result<HashMap<String, String>> {
    let Some(path) = path else {
        return Ok(HashMap::new());
    };
    let file = File::open(path)
        .with_context(|| format!("failed to open exclude CSV {}", path.display()))?;
    let mut reader = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .flexible(true)
        .from_reader(file);
    let mut map = HashMap::new();
    for result in reader.deserialize() {
        let row: ExcludeCsvRow = result
            .with_context(|| format!("failed to parse exclude CSV row in {}", path.display()))?;
        let number = row.phones.trim().to_string();
        if number.is_empty() {
            continue;
        }
        map.entry(number)
            .or_insert_with(|| row.label.trim().to_string());
    }
    Ok(map)
}

fn write_csv(path: &Path, contacts: &[ContactOut]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    writer.write_record([
        "phones",
        "first_name",
        "last_name",
        "exclude",
        "label_1",
        "label_2",
        "label_3",
        "label_4",
        "label_5",
    ])?;

    let mut truncated = 0u64;
    for c in contacts {
        if c.tags.len() > 5 {
            truncated += 1;
        }
        let phones = c.phones.join(";");
        let exclude = if c.exclude { "true" } else { "false" };
        let mut label_cols = [
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ];
        for (i, tag) in c.tags.iter().take(5).enumerate() {
            label_cols[i] = tag.clone();
        }
        writer.write_record([
            phones,
            c.first_name.clone(),
            c.last_name.clone(),
            exclude.to_string(),
            label_cols[0].clone(),
            label_cols[1].clone(),
            label_cols[2].clone(),
            label_cols[3].clone(),
            label_cols[4].clone(),
        ])?;
    }

    writer.flush()?;
    if truncated > 0 {
        eprintln!(
            "warning: truncated labels to label_1..label_5 for {truncated} contact(s) in CSV export"
        );
    }
    Ok(())
}
