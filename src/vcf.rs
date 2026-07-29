use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Default)]
pub struct VcfCard {
    pub fn_raw: String,
    pub n_family: String,
    pub n_given: String,
    pub n_middle: String,
    pub phones: Vec<String>,
    pub email: Option<String>,
    /// Values from `CATEGORIES` (and repeated CATEGORIES lines), already unescaped.
    pub categories: Vec<String>,
}

/// Parse a VCF 3.0 file into cards (unfolded lines).
pub fn parse_vcf(path: &Path) -> Result<Vec<VcfCard>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read VCF {}", path.display()))?;
    let lines = unfold_lines(&text);
    let mut cards = Vec::new();
    let mut current: Option<VcfCard> = None;

    for line in lines {
        if line.eq_ignore_ascii_case("BEGIN:VCARD") {
            current = Some(VcfCard::default());
            continue;
        }
        if line.eq_ignore_ascii_case("END:VCARD") {
            if let Some(card) = current.take() {
                cards.push(card);
            }
            continue;
        }
        let Some(card) = current.as_mut() else {
            continue;
        };
        apply_line(card, &line);
    }

    Ok(cards)
}

fn unfold_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
            }
            continue;
        }
        out.push(line.to_string());
    }
    out
}

fn apply_line(card: &mut VcfCard, line: &str) {
    let (name, value) = match line.split_once(':') {
        Some(parts) => parts,
        None => return,
    };
    let prop = name.split(';').next().unwrap_or(name);
    let prop_upper = prop.to_ascii_uppercase();

    // Strip itemN. prefix (e.g. item1.TEL, ITEM1.EMAIL)
    let base = prop_upper
        .rsplit_once('.')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or(prop_upper);

    match base.as_str() {
        "FN" => card.fn_raw = unescape(value),
        "N" => {
            let parts: Vec<&str> = value.split(';').collect();
            card.n_family = unescape(parts.first().copied().unwrap_or(""));
            card.n_given = unescape(parts.get(1).copied().unwrap_or(""));
            card.n_middle = unescape(parts.get(2).copied().unwrap_or(""));
        }
        "TEL" => {
            let phone = value.trim();
            if !phone.is_empty() && !card.phones.iter().any(|p| p == phone) {
                card.phones.push(phone.to_string());
            }
        }
        "EMAIL" => {
            if card.email.is_none() {
                let email = value.trim();
                if !email.is_empty() {
                    card.email = Some(email.to_string());
                }
            }
        }
        "CATEGORIES" => {
            for category in split_categories(value) {
                if !card
                    .categories
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&category))
                {
                    card.categories.push(category);
                }
            }
        }
        _ => {}
    }
}

/// Split a CATEGORIES value on unescaped commas, then unescape each token.
pub fn split_categories(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                cur.push('\\');
                cur.push(next);
            } else {
                cur.push('\\');
            }
            continue;
        }
        if ch == ',' {
            let token = unescape(&cur);
            if !token.is_empty() {
                out.push(token);
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    let token = unescape(&cur);
    if !token.is_empty() {
        out.push(token);
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
        .trim()
        .to_string()
}

/// Extract `[Tag]` values from a string and return (stripped_text, tags).
pub fn extract_tags(raw: &str) -> (String, Vec<String>) {
    let mut tags = Vec::new();
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            let mut tag = String::new();
            for c in chars.by_ref() {
                if c == ']' {
                    break;
                }
                tag.push(c);
            }
            let tag = tag.trim();
            if !tag.is_empty() {
                tags.push(tag.to_string());
            }
        } else {
            out.push(ch);
        }
    }
    let stripped = out.split_whitespace().collect::<Vec<_>>().join(" ");
    (stripped, tags)
}

pub fn strip_tags(raw: &str) -> String {
    extract_tags(raw).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn split_categories_handles_escaped_commas() {
        assert_eq!(
            split_categories(r"Family,Work\,Inc,Friends"),
            vec!["Family", "Work,Inc", "Friends"]
        );
        assert_eq!(split_categories("  Family  ,  "), vec!["Family"]);
        assert!(split_categories("").is_empty());
    }

    #[test]
    fn parse_vcf_collects_repeated_categories() {
        let mut tmp = tempfile::NamedTempFile::new().expect("temp");
        write!(
            tmp,
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nN:Lovelace;Ada;;;\n\
             TEL:+15551234567\nCATEGORIES:Family,Friends\nCATEGORIES:Work\n\
             CATEGORIES:family\nEND:VCARD\n"
        )
        .unwrap();
        let cards = parse_vcf(tmp.path()).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].categories,
            vec!["Family", "Friends", "Work"]
        );
    }
}
