//! Copy a contacts VCF/CSV and rewrite phones that [`normalize_checked`] accepts.

use crate::name::collapse_inner_whitespace;
use crate::vcard_csv::{VcardCsvColumns, normalize_vcard_csv_header};
use anyhow::{Context, Result, bail};
use phone::{PhoneRegion, normalize_checked};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Check-only or write-updates mode for the contacts-validate tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidateMode {
    /// Analyze only; do not write corrected files or a log file.
    Check,
    /// Write corrected copy (+ VCF for CSV inputs) and log beside the input.
    #[default]
    Update,
}

/// Full result of a validate run.
#[derive(Debug, Default)]
pub struct ValidateReport {
    /// Count of phones rewritten to a certain E.164.
    pub rewritten: u64,
    /// Count of phones left unchanged as uncertain.
    pub uncertain: u64,
    /// Count of E.164 values shared by more than one contact.
    pub duplicate_groups: u64,
    /// Planned or written primary output path.
    pub output_path: PathBuf,
    /// Companion VCF (CSV inputs only).
    pub vcf_path: Option<PathBuf>,
    /// Planned or written log path.
    pub log_path: PathBuf,
    /// True when files were written (`Update` mode).
    pub wrote_files: bool,
    /// Full validate.log contents (also returned for Check so UIs can display them).
    pub log_lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct OutCard {
    fn_raw: String,
    n_family: String,
    n_given: String,
    phones: Vec<String>,
}

#[derive(Debug, Clone)]
struct UnableEntry {
    contact: String,
    phone: String,
    reason: String,
}

/// Formats accepted by contacts-validate and by [`crate::ContactsBook::load_contacts_file`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactsFormat {
    /// vCard `.vcf`/`.vcard` input.
    Vcf,
    /// First/Last Name plus phone-column CSV input.
    VcardCsv,
}

/// Short red-box message when CSV/VCF content is not a known contacts format.
const UNRECOGNIZED_CONTACTS_FORMAT: &str = "Unrecognized contacts format.";

/// Probe failure for GUI preflight (short `message` + optional log `details`).
#[derive(Debug, Clone)]
pub struct ContactsInputError {
    /// Short human-readable error (e.g. `"Unrecognized contacts format."`).
    pub message: String,
    /// Optional verbose detail lines for logs.
    pub details: Vec<String>,
}

impl std::fmt::Display for ContactsInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.details.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{} ({})", self.message, self.details.join("; "))
        }
    }
}

impl std::error::Error for ContactsInputError {}

impl ContactsInputError {
    fn simple(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: Vec::new(),
        }
    }

    fn unrecognized(details: Vec<String>) -> Self {
        Self {
            message: UNRECOGNIZED_CONTACTS_FORMAT.into(),
            details,
        }
    }
}

/// Validate contacts beside `input`.
///
/// - [`ValidateMode::Update`]: write `{stem}-update.{ext}` (or `{stem}-update-N` when the
///   input already ends in `-update` / `-update-N`) plus `.log`; CSV also writes `.vcf`.
/// - [`ValidateMode::Check`]: same analysis; no files written; results are in [`ValidateReport::log_lines`].
///
/// # Errors
///
/// Returns an error when the input is missing, the format is unknown, or a
/// rewrite file cannot be written.
pub fn validate_contacts_file(
    input: &Path,
    region: PhoneRegion,
    mode: ValidateMode,
) -> Result<ValidateReport> {
    if !input.is_file() {
        bail!("input not found: {}", input.display());
    }

    let write = mode == ValidateMode::Update;
    let format = detect_format(input)?;
    let (output_path, log_path) = corrected_output_paths(input);

    let mode_label = match mode {
        ValidateMode::Check => "check",
        ValidateMode::Update => "format",
    };
    let mut acc = ValidateAccum::default();
    acc.log_lines.push(format!(
        "# contacts-validate mode={mode_label} region={region}"
    ));
    acc.log_lines.push(format!("# input={}", input.display()));
    acc.log_lines.push(String::new());

    let mut cards: Vec<OutCard> = Vec::new();

    match format {
        ContactsFormat::Vcf => {
            rewrite_vcf(RewriteVcfArgs {
                input,
                output: &output_path,
                region,
                write,
                acc: &mut acc,
            })?;
        }
        ContactsFormat::VcardCsv => {
            rewrite_vcard_csv(RewriteVcardCsvArgs {
                input,
                output: &output_path,
                region,
                write,
                acc: &mut acc,
                cards: &mut cards,
            })?;
        }
    }

    let vcf_path = if matches!(format, ContactsFormat::VcardCsv) {
        let vcf_path = output_path.with_extension("vcf");
        if write {
            write_vcf_cards(&vcf_path, &cards)?;
        }
        Some(vcf_path)
    } else {
        None
    };

    let unable = std::mem::take(&mut acc.unable);
    emit_uncertain_sections(&mut acc.log_lines, &unable);

    let mut duplicate_groups = 0u64;
    acc.log_lines.push(String::new());
    acc.log_lines
        .push("Duplicate numbers (same E.164 on more than one contact)".into());
    let mut keys: Vec<_> = acc.by_e164.keys().cloned().collect();
    keys.sort();
    let mut any_dup = false;
    for e164 in keys {
        let names = acc.by_e164.get(&e164).cloned().unwrap_or_default();
        if names.len() > 1 {
            duplicate_groups += 1;
            any_dup = true;
            acc.log_lines
                .push(format!("  {e164}: {}", names.join(" | ")));
        }
    }
    if !any_dup {
        acc.log_lines.push("  (none)".into());
    }

    let file_written = if write {
        output_path.display().to_string()
    } else {
        "none".into()
    };
    acc.log_lines.push(String::new());
    acc.log_lines.push("Summary".into());
    acc.log_lines
        .push(format!("  - Numbers formatted: {}", acc.rewritten));
    acc.log_lines
        .push(format!("  - Uncertain: {}", acc.uncertain));
    acc.log_lines
        .push(format!("  - Duplicates: {duplicate_groups}"));
    acc.log_lines.push(format!("  - Mode: {mode_label}"));
    acc.log_lines
        .push(format!("    - File written: {file_written}"));

    if write {
        let mut log_file =
            File::create(&log_path).with_context(|| format!("create {}", log_path.display()))?;
        for line in &acc.log_lines {
            writeln!(log_file, "{line}")?;
        }
    }

    Ok(ValidateReport {
        rewritten: acc.rewritten,
        uncertain: acc.uncertain,
        duplicate_groups,
        output_path,
        vcf_path,
        log_path,
        wrote_files: write,
        log_lines: acc.log_lines,
    })
}

/// Counters, log lines, and duplicate/uncertain accumulators threaded through
/// every rewrite pass as one `&mut`.
#[derive(Debug, Default)]
struct ValidateAccum {
    /// Count of phones rewritten to a certain E.164.
    rewritten: u64,
    /// Count of phones left unchanged as uncertain.
    uncertain: u64,
    /// Full validate.log contents.
    log_lines: Vec<String>,
    /// Phones that could not be made certain, grouped later by reason.
    unable: Vec<UnableEntry>,
    /// e164 → list of contact labels (with row) that own it (after rewrite).
    by_e164: HashMap<String, Vec<String>>,
}

fn corrected_output_paths(input: &Path) -> (PathBuf, PathBuf) {
    let parent = input.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("contacts");
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("csv");
    let base = update_output_stem(stem);
    (
        parent.join(format!("{base}.{ext}")),
        parent.join(format!("{base}.log")),
    )
}

/// `contacts` → `contacts-update`; `contacts-update` → `contacts-update-2`;
/// `contacts-update-N` → `contacts-update-(N+1)`.
fn update_output_stem(stem: &str) -> String {
    if let Some(prefix) = stem.strip_suffix("-update") {
        return format!("{prefix}-update-2");
    }
    if let Some((prefix, n_str)) = stem.rsplit_once("-update-")
        && let Ok(n) = n_str.parse::<u32>()
        && n >= 2
        && !n_str.is_empty()
        && n_str.chars().all(|c| c.is_ascii_digit())
    {
        return format!("{prefix}-update-{}", n + 1);
    }
    format!("{stem}-update")
}

/// Detect VCF or vCard CSV (First Name, Last Name, phone columns).
///
/// # Errors
///
/// Returns a `ContactsInputError` when the path is missing, the extension is
/// unknown, or the content is not a recognized contacts format.
pub fn detect_contacts_format(path: &Path) -> Result<ContactsFormat, ContactsInputError> {
    detect_format(path)
}

fn detect_format(path: &Path) -> Result<ContactsFormat, ContactsInputError> {
    if !path.is_file() {
        return Err(ContactsInputError::simple("Contacts file not found"));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "csv" && ext != "vcf" && ext != "vcard" {
        return Err(ContactsInputError::simple(format!(
            "Contacts file must be a .csv or .vcf file: {}",
            path.display()
        )));
    }
    if ext == "vcf" || ext == "vcard" {
        return detect_vcf_format(path);
    }
    detect_csv_format(path)
}

fn detect_vcf_format(path: &Path) -> Result<ContactsFormat, ContactsInputError> {
    let text = fs::read_to_string(path).map_err(|e| {
        ContactsInputError::simple(format!("Could not read {}: {e}", path.display()))
    })?;
    let mut has_begin = false;
    let mut has_end = false;
    for line in text.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("BEGIN:VCARD") {
            has_begin = true;
        } else if t.eq_ignore_ascii_case("END:VCARD") {
            has_end = true;
        }
        if has_begin && has_end {
            return Ok(ContactsFormat::Vcf);
        }
    }
    let mut details = vec![format!("file={}", path.display())];
    if !has_begin {
        details.push("missing BEGIN:VCARD".into());
    }
    if !has_end {
        details.push("missing END:VCARD".into());
    }
    details.push("expected at least one BEGIN:VCARD … END:VCARD block".into());
    Err(ContactsInputError::unrecognized(details))
}

fn detect_csv_format(path: &Path) -> Result<ContactsFormat, ContactsInputError> {
    let file = File::open(path).map_err(|e| {
        ContactsInputError::simple(format!("Could not open {}: {e}", path.display()))
    })?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(file);
    let headers = rdr.headers().map_err(|e| {
        ContactsInputError::unrecognized(vec![
            format!("file={}", path.display()),
            format!("could not read CSV header: {e}"),
        ])
    })?;
    let header_l: Vec<String> = headers.iter().map(normalize_vcard_csv_header).collect();
    let cols = VcardCsvColumns::from_headers(headers.iter());
    let has_first = cols.first.is_some();
    let has_last = cols.last.is_some();
    let phone_cols: Vec<&str> = cols
        .phones
        .iter()
        .filter_map(|&i| header_l.get(i).map(String::as_str))
        .collect();
    let has_phone = !phone_cols.is_empty();

    if has_first && has_last && has_phone {
        return Ok(ContactsFormat::VcardCsv);
    }

    let mut details = vec![
        format!("file={}", path.display()),
        format!("headers={}", header_l.join(" | ")),
    ];
    if !has_first {
        details.push("missing First Name column".into());
    }
    if !has_last {
        details.push("missing Last Name column".into());
    }
    if !has_phone {
        details.push("missing Phone column (Mobile Phone, Home Phone, …)".into());
    } else {
        details.push(format!("phone columns: {}", phone_cols.join(", ")));
    }
    details.push(
        "valid CSV needs First Name, Last Name, and at least one Phone column \
         (vCard CSV)"
            .into(),
    );
    Err(ContactsInputError::unrecognized(details))
}

/// Label used in duplicate listings (always includes CSV/VCF row index).
fn contact_label(row: u64, name: &str) -> String {
    let name = collapse_inner_whitespace(name);
    if name.is_empty() || name == "(unnamed)" {
        format!("row {row}")
    } else {
        format!("row {row}: {name}")
    }
}

/// Display name for uncertain-format lines (name, or `row N` when unnamed).
fn contact_display_name(row: u64, name: &str) -> String {
    let name = collapse_inner_whitespace(name);
    if name.is_empty() || name == "(unnamed)" {
        format!("row {row}")
    } else {
        name
    }
}

fn emit_uncertain_sections(log_lines: &mut Vec<String>, unable: &[UnableEntry]) {
    log_lines.push(String::new());
    if unable.is_empty() {
        return;
    }
    let mut by_reason: HashMap<String, Vec<&UnableEntry>> = HashMap::new();
    for entry in unable {
        by_reason
            .entry(entry.reason.clone())
            .or_default()
            .push(entry);
    }
    let mut reasons: Vec<_> = by_reason.keys().cloned().collect();
    reasons.sort();
    for reason in reasons {
        log_lines.push(format!("UNCERTAIN FORMAT - {reason}"));
        for entry in &by_reason[&reason] {
            log_lines.push(format!("  - {:?} - {:?}", entry.contact, entry.phone));
        }
    }
}

struct RewritePhoneTokenArgs<'a> {
    raw: &'a str,
    /// Label for duplicate tracking (includes row).
    contact_dup: &'a str,
    /// Name shown under UNCERTAIN FORMAT (name or `row N`).
    contact_uncertain: &'a str,
    region: PhoneRegion,
    acc: &'a mut ValidateAccum,
    log_success: bool,
}

fn rewrite_phone_token(args: RewritePhoneTokenArgs<'_>) -> String {
    let RewritePhoneTokenArgs {
        raw,
        contact_dup,
        contact_uncertain,
        region,
        acc,
        log_success,
    } = args;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    match normalize_checked(trimmed, region) {
        Ok(e164) => {
            acc.rewritten += 1;
            acc.by_e164
                .entry(e164.clone())
                .or_default()
                .push(contact_dup.to_string());
            if log_success {
                if e164 == trimmed {
                    acc.log_lines
                        .push(format!("TEL contact={contact_dup:?} phone={e164}"));
                } else {
                    acc.log_lines.push(format!(
                        "REWRITTEN contact={contact_dup:?} phone={trimmed:?} -> {e164}"
                    ));
                }
            }
            e164
        }
        Err(reason) => {
            acc.uncertain += 1;
            acc.unable.push(UnableEntry {
                contact: contact_uncertain.to_string(),
                phone: trimmed.to_string(),
                reason,
            });
            raw.to_string()
        }
    }
}

struct RewritePhoneListArgs<'a> {
    raw: &'a str,
    contact_dup: &'a str,
    contact_uncertain: &'a str,
    region: PhoneRegion,
    sep: char,
    acc: &'a mut ValidateAccum,
}

fn rewrite_phone_list(args: RewritePhoneListArgs<'_>) -> String {
    let RewritePhoneListArgs {
        raw,
        contact_dup,
        contact_uncertain,
        region,
        sep,
        acc,
    } = args;
    if raw.trim().is_empty() {
        return raw.to_string();
    }
    let parts: Vec<String> = raw
        .split(sep)
        .map(|p| {
            rewrite_phone_token(RewritePhoneTokenArgs {
                raw: p,
                contact_dup,
                contact_uncertain,
                region,
                acc,
                log_success: false,
            })
        })
        .collect();
    parts.join(&sep.to_string())
}

struct RewriteVcfArgs<'a> {
    input: &'a Path,
    output: &'a Path,
    region: PhoneRegion,
    write: bool,
    acc: &'a mut ValidateAccum,
}

fn rewrite_vcf(args: RewriteVcfArgs<'_>) -> Result<()> {
    let RewriteVcfArgs {
        input,
        output,
        region,
        write,
        acc,
    } = args;
    let text = fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;
    let mut out = String::new();
    let mut current_name = String::from("(unnamed)");
    let mut card_index = 0u64;
    for line in text.lines() {
        let trimmed = line.trim_end();
        let upper = trimmed.to_ascii_uppercase();
        if upper == "BEGIN:VCARD" {
            card_index += 1;
            current_name = "(unnamed)".into();
            acc.log_lines.push(format!("# vcard {card_index} begin"));
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if upper == "END:VCARD" {
            acc.log_lines
                .push(format!("# vcard {card_index} end name={current_name:?}"));
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if upper.starts_with("FN:") {
            current_name = trimmed[3..].trim().to_string();
            if current_name.is_empty() {
                current_name = "(unnamed)".into();
            }
            acc.log_lines
                .push(format!("# vcard {card_index} FN={current_name:?}"));
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if upper.starts_with("N:") && current_name == "(unnamed)" {
            // fallback name from N if FN missing
            let n = &trimmed[2..];
            let parts: Vec<&str> = n.split(';').collect();
            let family = parts.first().copied().unwrap_or("").trim();
            let given = parts.get(1).copied().unwrap_or("").trim();
            current_name = collapse_inner_whitespace(&format!("{given} {family}"));
            if current_name.is_empty() {
                current_name = "(unnamed)".into();
            }
            acc.log_lines
                .push(format!("# vcard {card_index} N={current_name:?}"));
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // TEL;TYPE=CELL:+1-555… or TEL:+1…
        if upper.starts_with("TEL")
            && let Some((prefix, value)) = trimmed.split_once(':')
        {
            let label = contact_label(card_index, &current_name);
            let display = contact_display_name(card_index, &current_name);
            let new_val = rewrite_phone_token(RewritePhoneTokenArgs {
                raw: value,
                contact_dup: &label,
                contact_uncertain: &display,
                region,
                acc,
                log_success: true,
            });
            out.push_str(prefix);
            out.push(':');
            out.push_str(&new_val);
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if write {
        fs::write(output, out).with_context(|| format!("write {}", output.display()))?;
    }
    Ok(())
}

fn write_vcf_cards(path: &Path, cards: &[OutCard]) -> Result<()> {
    let mut out = String::new();
    for card in cards {
        if card.phones.is_empty() && card.fn_raw.is_empty() {
            continue;
        }
        out.push_str("BEGIN:VCARD\n");
        out.push_str("VERSION:3.0\n");
        out.push_str(&format!(
            "N:{};{};;;\n",
            vcf_escape(&card.n_family),
            vcf_escape(&card.n_given)
        ));
        if !card.fn_raw.is_empty() {
            out.push_str(&format!("FN:{}\n", vcf_escape(&card.fn_raw)));
        }
        for phone in &card.phones {
            if !phone.trim().is_empty() {
                out.push_str(&format!("TEL:{}\n", phone.trim()));
            }
        }
        out.push_str("END:VCARD\n");
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn vcf_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

struct RewriteVcardCsvArgs<'a> {
    input: &'a Path,
    output: &'a Path,
    region: PhoneRegion,
    write: bool,
    acc: &'a mut ValidateAccum,
    cards: &'a mut Vec<OutCard>,
}

fn rewrite_vcard_csv(args: RewriteVcardCsvArgs<'_>) -> Result<()> {
    let RewriteVcardCsvArgs {
        input,
        output,
        region,
        write,
        acc,
        cards,
    } = args;
    let file = File::open(input).with_context(|| format!("open {}", input.display()))?;
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(file);
    let headers = rdr.headers()?.clone();
    let cols = VcardCsvColumns::from_headers(headers.iter());

    let mut wtr = if write {
        let out_file =
            File::create(output).with_context(|| format!("create {}", output.display()))?;
        let mut wtr = csv::Writer::from_writer(out_file);
        wtr.write_record(&headers)?;
        Some(wtr)
    } else {
        None
    };

    for (idx, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("row {}", idx + 2))?;
        let first = cols.first.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let middle = cols.middle.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let last = cols.last.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let mut parts = Vec::new();
        if !first.is_empty() {
            parts.push(first);
        }
        if !middle.is_empty() {
            parts.push(middle);
        }
        if !last.is_empty() {
            parts.push(last);
        }
        let name = collapse_inner_whitespace(&parts.join(" "));
        let row = (idx + 2) as u64;
        let contact_dup = contact_label(row, &name);
        let contact_uncertain = contact_display_name(row, &name);

        let mut fields: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
        while fields.len() < headers.len() {
            fields.push(String::new());
        }
        let mut phones = Vec::new();
        for &i in &cols.phones {
            if let Some(cell) = fields.get_mut(i) {
                if cell.trim().is_empty() {
                    continue;
                }
                // Some exporters pack multiple phones with `;`
                *cell = rewrite_phone_list(RewritePhoneListArgs {
                    raw: cell,
                    contact_dup: &contact_dup,
                    contact_uncertain: &contact_uncertain,
                    region,
                    sep: ';',
                    acc,
                });
                for p in cell.split(';') {
                    let p = p.trim();
                    if !p.is_empty() && !phones.iter().any(|x| x == p) {
                        phones.push(p.to_string());
                    }
                }
            }
        }
        if !phones.is_empty() || !name.is_empty() {
            cards.push(OutCard {
                fn_raw: name,
                n_family: last.to_string(),
                n_given: first.to_string(),
                phones,
            });
        }
        if let Some(wtr) = wtr.as_mut() {
            wtr.write_record(&fields)?;
        }
    }
    if let Some(mut wtr) = wtr {
        wtr.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn detect_rejects_missing_wrong_ext_and_bad_format() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.csv");
        let err = detect_contacts_format(&missing).unwrap_err();
        assert!(err.message.contains("not found"));

        let wrong_ext = write(&dir, "contacts.txt", "BEGIN:VCARD\nEND:VCARD\n");
        let err = detect_contacts_format(&wrong_ext).unwrap_err();
        assert!(err.message.contains("must be a .csv or .vcf"));

        let bad_csv = write(&dir, "contacts.csv", "name,phone\nAda,123\n");
        let err = detect_contacts_format(&bad_csv).unwrap_err();
        assert_eq!(err.message, UNRECOGNIZED_CONTACTS_FORMAT);
        assert!(err.details.iter().any(|d| d.contains("First Name")));
        assert!(err.details.iter().any(|d| d.contains("Last Name")));

        let missing_last = write(
            &dir,
            "partial.csv",
            "First Name,Mobile Phone\nAda,+15551234567\n",
        );
        let err = detect_contacts_format(&missing_last).unwrap_err();
        assert_eq!(err.message, UNRECOGNIZED_CONTACTS_FORMAT);
        assert!(err.details.iter().any(|d| d.contains("Last Name")));

        let empty_vcf = write(&dir, "empty.vcf", "NOTE: not a vcard\n");
        let err = detect_contacts_format(&empty_vcf).unwrap_err();
        assert_eq!(err.message, UNRECOGNIZED_CONTACTS_FORMAT);
        assert!(err.details.iter().any(|d| d.contains("BEGIN:VCARD")));

        let vcard_csv = write(
            &dir,
            "vcard.csv",
            "First Name,Last Name,Mobile Phone\nAda,Lovelace,+15551234567\n",
        );
        assert_eq!(
            detect_contacts_format(&vcard_csv).unwrap(),
            ContactsFormat::VcardCsv
        );

        let vcf = write(&dir, "ok.vcf", "BEGIN:VCARD\nFN:Ada\nEND:VCARD\n");
        assert_eq!(detect_contacts_format(&vcf).unwrap(), ContactsFormat::Vcf);
    }

    #[test]
    fn vcf_validate_logs_card_and_phone_lines() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "c.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\n\
TEL;TYPE=CELL:+44 20 7183 8750\n\
TEL;TYPE=HOME:(542).341-2398\n\
END:VCARD\n",
        );
        let report =
            validate_contacts_file(&input, PhoneRegion::International, ValidateMode::Check)
                .unwrap();
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.contains("# vcard 1 begin"))
        );
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.contains("FN=\"Ada Lovelace\""))
        );
        assert!(report.log_lines.iter().any(|l| l.contains("REWRITTEN")));
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.starts_with("UNCERTAIN FORMAT - "))
        );
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.contains("\"Ada Lovelace\" - "))
        );
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.contains("# vcard 1 end name=\"Ada Lovelace\""))
        );
        assert!(report.log_lines.iter().any(|l| l == "Summary"));
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l == "  - Numbers formatted: 1")
        );
        assert!(report.log_lines.iter().any(|l| l == "  - Uncertain: 1"));
        assert!(report.log_lines.iter().any(|l| l == "  - Mode: check"));
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l == "    - File written: none")
        );
    }

    #[test]
    fn validates_vcard_csv_usa() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "contacts.csv",
            "First Name,Last Name,Mobile Phone\n\
Ada,Lovelace,(542).341-2398\n\
Short,Number,1555-4567\n\
Clone,Contact,(542).341-2398\n",
        );
        let report =
            validate_contacts_file(&input, PhoneRegion::Usa, ValidateMode::Update).unwrap();
        assert!(report.wrote_files);
        assert_eq!(report.rewritten, 2);
        assert_eq!(report.uncertain, 1);
        assert_eq!(report.duplicate_groups, 1);
        let name = report.output_path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "contacts-update.csv");
        assert_eq!(report.output_path.parent(), Some(dir.path()));
        assert_eq!(
            report.log_path.extension().and_then(|e| e.to_str()),
            Some("log")
        );
        let body = fs::read_to_string(&report.output_path).unwrap();
        assert!(body.contains("+15423412398"));
        assert!(body.contains("1555-4567"));
        let vcf_path = report.vcf_path.expect("csv should also write vcf");
        assert!(vcf_path.extension().is_some_and(|e| e == "vcf"));
        let vcf = fs::read_to_string(&vcf_path).unwrap();
        assert!(vcf.contains("BEGIN:VCARD"));
        assert!(vcf.contains("FN:Ada Lovelace"));
        assert!(vcf.contains("TEL:+15423412398"));
        let log = fs::read_to_string(&report.log_path).unwrap();
        assert!(log.contains("UNCERTAIN FORMAT - "));
        assert!(log.contains("Duplicate numbers (same E.164 on more than one contact)"));
        assert!(log.contains("row 2: Ada Lovelace"));
        assert!(log.contains("row 4: Clone Contact"));
        assert!(log.contains("Summary"));
        assert!(log.contains("  - Numbers formatted: 2"));
        assert!(log.contains("  - Uncertain: 1"));
        assert!(log.contains("  - Duplicates: 1"));
        assert!(log.contains("  - Mode: format"));
        assert!(log.contains("    - File written: "));
    }

    #[test]
    fn check_does_not_write_files() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "contacts.csv",
            "First Name,Last Name,Mobile Phone\nAda,Lovelace,(542).341-2398\nShort,Number,1555-4567\n",
        );
        let before: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let report = validate_contacts_file(&input, PhoneRegion::Usa, ValidateMode::Check).unwrap();
        assert!(!report.wrote_files);
        assert_eq!(report.rewritten, 1);
        assert_eq!(report.uncertain, 1);
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l.starts_with("UNCERTAIN FORMAT - "))
        );
        assert!(report.log_lines.iter().any(|l| l == "Summary"));
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l == "  - Numbers formatted: 1")
        );
        assert!(report.log_lines.iter().any(|l| l == "  - Mode: check"));
        assert!(
            report
                .log_lines
                .iter()
                .any(|l| l == "    - File written: none")
        );
        let after: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(before.len(), after.len(), "check must not create files");
        assert!(!report.output_path.exists());
    }

    #[test]
    fn validates_vcf_international() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "c.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\n\
TEL;TYPE=CELL:+44 20 7183 8750\n\
TEL;TYPE=HOME:(542).341-2398\n\
END:VCARD\n",
        );
        let report =
            validate_contacts_file(&input, PhoneRegion::International, ValidateMode::Update)
                .unwrap();
        assert_eq!(report.rewritten, 1);
        assert_eq!(report.uncertain, 1);
        let name = report.output_path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "c-update.vcf");
        let body = fs::read_to_string(&report.output_path).unwrap();
        assert!(body.contains("+442071838750"));
        assert!(body.contains("(542).341-2398"));
    }

    #[test]
    fn update_stem_increments_when_input_already_updated() {
        assert_eq!(update_output_stem("contacts"), "contacts-update");
        assert_eq!(update_output_stem("contacts-update"), "contacts-update-2");
        assert_eq!(update_output_stem("contacts-update-2"), "contacts-update-3");
        assert_eq!(update_output_stem("foo-update-9"), "foo-update-10");
    }
}
