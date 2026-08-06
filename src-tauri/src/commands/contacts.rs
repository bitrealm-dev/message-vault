//! `contacts_info` Tauri command — parses a VCF or vCard CSV file and
//! returns the contact count and first few names for preview.

use std::path::PathBuf;

use contacts::{detect_contacts_format, parse_vcf, read_vcard_csv_rows, ContactsFormat};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ContactCard {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactsInfo {
    pub count: usize,
    pub format: String,
    pub preview: Vec<String>,
    pub cards: Vec<ContactCard>,
}

#[tauri::command]
pub async fn contacts_info(path: String) -> Result<ContactsInfo, String> {
    let path = PathBuf::from(&path);
    let format = detect_contacts_format(&path).map_err(|e| e.to_string())?;

    match format {
        ContactsFormat::Vcf => {
            let cards = parse_vcf(&path).map_err(|e| e.to_string())?;
            let preview: Vec<String> = cards
                .iter()
                .take(10)
                .map(|c| {
                    if !c.fn_raw.is_empty() {
                        c.fn_raw.clone()
                    } else {
                        let name = format!("{} {}", c.n_given, c.n_family).trim().to_string();
                        if name.is_empty() { "(unknown)".to_string() } else { name }
                    }
                })
                .collect();
            let contact_cards: Vec<ContactCard> = cards
                .iter()
                .map(|c| {
                    let name = if !c.fn_raw.is_empty() {
                        c.fn_raw.clone()
                    } else {
                        let n = format!("{} {}", c.n_given, c.n_family).trim().to_string();
                        if n.is_empty() { "(unknown)".to_string() } else { n }
                    };
                    ContactCard {
                        name,
                        phone: c.phones.first().cloned(),
                        email: c.email.clone(),
                    }
                })
                .collect();
            Ok(ContactsInfo {
                count: cards.len(),
                format: "vcf".to_string(),
                preview,
                cards: contact_cards,
            })
        }
        ContactsFormat::VcardCsv => {
            let rows = read_vcard_csv_rows(&path).map_err(|e| e.to_string())?;
            let preview: Vec<String> = rows
                .iter()
                .take(10)
                .map(|r| {
                    let name = format!("{} {} {}", r.first, r.middle, r.last)
                        .trim()
                        .to_string();
                    if name.is_empty() { "(unknown)".to_string() } else { name }
                })
                .collect();
            let contact_cards: Vec<ContactCard> = rows
                .iter()
                .map(|r| {
                    let name = format!("{} {} {}", r.first, r.middle, r.last)
                        .trim()
                        .to_string();
                    ContactCard {
                        name: if name.is_empty() { "(unknown)".to_string() } else { name },
                        phone: r.phones.first().cloned(),
                        email: None,
                    }
                })
                .collect();
            Ok(ContactsInfo {
                count: rows.len(),
                format: "csv".to_string(),
                preview,
                cards: contact_cards,
            })
        }
    }
}
