//! Shared US-centric phone normalization for message-vault-io.

use std::collections::HashSet;
use std::fmt;

use anyhow::{Context, Result, bail};
use message_ir::HandleType;

/// Minimum digit length after stripping formatting.
///
/// Allows 4–6 digit short codes (carrier/bank SMS, e.g. AT&T `7535`).
/// Rejects junk like `"4"` or `"06"`.
const MIN_PHONE_DIGITS: usize = 4;

/// Region rules for [`normalize_certain`] (contacts validation only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneRegion {
    /// US NANP: certain only for 10 digits or 11 digits starting with `1`.
    Usa,
    /// International: certain only when the raw value has a leading `+`.
    International,
}

impl fmt::Display for PhoneRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhoneRegion::Usa => write!(f, "usa"),
            PhoneRegion::International => write!(f, "international"),
        }
    }
}

impl PhoneRegion {
    pub fn parse_cli(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "usa" | "us" => Some(Self::Usa),
            "international" | "intl" => Some(Self::International),
            _ => None,
        }
    }
}

/// Strip non-digits and a leading US country code `1`.
/// Returns `None` when fewer than [`MIN_PHONE_DIGITS`] remain.
pub fn sanitize_number(num: &str) -> Option<String> {
    if num.is_empty() {
        return None;
    }
    let mut digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits = digits[1..].to_string();
    }
    if digits.len() < MIN_PHONE_DIGITS {
        None
    } else {
        Some(digits)
    }
}

/// Format already-sanitized digits as E.164 (`+1…` for 10-digit US).
///
/// Pass the output of [`sanitize_number`], not a raw user string.
pub fn to_e164(digits: &str) -> String {
    if digits.len() == 10 {
        format!("+1{digits}")
    } else if digits.starts_with('+') {
        digits.to_string()
    } else {
        format!("+{digits}")
    }
}

/// Canonical E.164 only when the parse is unambiguous for `region`.
///
/// Unlike [`sanitize_number`], this does **not** accept short codes or
/// ambiguous lengths — used by contacts validation before rewriting files.
pub fn normalize_certain(raw: &str, region: PhoneRegion) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    match region {
        PhoneRegion::Usa => {
            if digits.len() == 10 {
                Some(format!("+1{digits}"))
            } else if digits.len() == 11 && digits.starts_with('1') {
                Some(format!("+{digits}"))
            } else {
                None
            }
        }
        PhoneRegion::International => {
            if !raw.contains('+') {
                return None;
            }
            if (8..=15).contains(&digits.len()) {
                Some(format!("+{digits}"))
            } else {
                None
            }
        }
    }
}

/// Human-readable reason when [`normalize_certain`] returns `None`.
pub fn normalize_uncertain_reason(raw: &str, region: PhoneRegion) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "empty phone".into();
    }
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    match region {
        PhoneRegion::Usa => {
            if digits.len() == 10 || (digits.len() == 11 && digits.starts_with('1')) {
                "unexpected: looked certain".into()
            } else if digits.is_empty() {
                "no digits".into()
            } else {
                // Fixed string so validate log groups all USA length failures under one header.
                "USA needs 10 digits or 11 starting with 1".into()
            }
        }
        PhoneRegion::International => {
            if !raw.contains('+') {
                "international mode requires a leading +".into()
            } else if !(8..=15).contains(&digits.len()) {
                "international needs 8–15 digits after +".into()
            } else {
                "unexpected: looked certain".into()
            }
        }
    }
}

/// All configured owner handles (normalized, typed).
#[derive(Debug, Clone)]
pub struct OwnerHandleSet {
    handles: HashSet<(String, HandleType)>,
}

impl OwnerHandleSet {
    pub fn new(handles: &[(String, HandleType)]) -> Result<Self> {
        if handles.is_empty() {
            bail!("owner handle required: pass --owner-phone or --owner-handle");
        }
        let mut set = HashSet::new();
        for (raw, handle_type) in handles {
            let normalized = match handle_type {
                HandleType::Phone => {
                    let d = sanitize_number(raw)
                        .with_context(|| format!("owner phone has no usable digits: {raw}"))?;
                    to_e164(&d)
                }
                HandleType::Email => raw.trim().to_lowercase(),
                HandleType::Username | HandleType::Other => raw.trim().to_string(),
            };
            set.insert((normalized, *handle_type));
        }
        Ok(Self { handles: set })
    }

    pub fn is_owner(&self, raw: &str, handle_type: HandleType) -> bool {
        let normalized = match handle_type {
            HandleType::Phone => {
                let Some(d) = sanitize_number(raw) else {
                    return false;
                };
                to_e164(&d)
            }
            HandleType::Email => raw.trim().to_lowercase(),
            HandleType::Username | HandleType::Other => raw.trim().to_string(),
        };
        self.handles.contains(&(normalized, handle_type))
    }

    /// Convenience for exporters that only know about phone numbers.
    pub fn from_phones(phones: &[String]) -> Result<Self> {
        let handles: Vec<(String, HandleType)> = phones
            .iter()
            .map(|p| (p.clone(), HandleType::Phone))
            .collect();
        Self::new(&handles)
    }
}

/// All configured owner phone numbers (normalized digits).
///
/// Deprecated: use [`OwnerHandleSet`], which also covers email and username
/// handles. Kept as a compatibility wrapper for exporters that haven't
/// migrated yet.
#[derive(Debug, Clone)]
#[deprecated(note = "use OwnerHandleSet")]
pub struct OwnerPhoneSet {
    pub all_digits: HashSet<String>,
    pub primary_digits: String,
    handles: OwnerHandleSet,
}

#[allow(deprecated)]
impl OwnerPhoneSet {
    pub fn new(phones: &[String]) -> Result<Self> {
        if phones.is_empty() {
            bail!("owner phone required: pass --owner-phone (or set phones in config)");
        }
        let mut all_digits = HashSet::new();
        for phone in phones {
            let d = sanitize_number(phone)
                .with_context(|| format!("owner phone has no usable digits: {phone}"))?;
            all_digits.insert(d);
        }
        let primary_digits =
            sanitize_number(&phones[0]).context("owner phone has no usable digits")?;
        let handles = OwnerHandleSet::from_phones(phones)?;
        Ok(Self {
            all_digits,
            primary_digits,
            handles,
        })
    }

    pub fn is_owner(&self, digits: &str) -> bool {
        self.handles.is_owner(digits, HandleType::Phone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_plus_one() {
        assert_eq!(
            sanitize_number("+15555550100").as_deref(),
            Some("5555550100")
        );
        assert_eq!(
            sanitize_number("(555) 555-0101").as_deref(),
            Some("5555550101")
        );
        assert_eq!(sanitize_number(""), None);
        assert_eq!(sanitize_number("4"), None);
        assert_eq!(sanitize_number("06"), None);
    }

    #[test]
    fn sanitize_keeps_short_codes() {
        assert_eq!(sanitize_number("7535").as_deref(), Some("7535"));
        assert_eq!(sanitize_number("73737").as_deref(), Some("73737"));
        assert_eq!(to_e164("7535"), "+7535");
        assert_eq!(to_e164("73737"), "+73737");
    }

    #[test]
    fn e164_us() {
        assert_eq!(to_e164("5555550100"), "+15555550100");
    }

    #[test]
    fn certain_usa() {
        assert_eq!(
            normalize_certain("(542).341-2398", PhoneRegion::Usa).as_deref(),
            Some("+15423412398")
        );
        assert_eq!(
            normalize_certain("1-555-456-7890", PhoneRegion::Usa).as_deref(),
            Some("+15554567890")
        );
        assert_eq!(
            normalize_certain("1555-4567", PhoneRegion::Usa),
            None,
            "too short for USA certainty"
        );
        assert_eq!(normalize_certain("+442071838750", PhoneRegion::Usa), None);
    }

    #[test]
    fn certain_international() {
        assert_eq!(
            normalize_certain("+44 20 7183 8750", PhoneRegion::International).as_deref(),
            Some("+442071838750")
        );
        assert_eq!(
            normalize_certain("(542).341-2398", PhoneRegion::International),
            None,
            "no leading +"
        );
        assert_eq!(
            normalize_certain("+1-542-341-2398", PhoneRegion::International).as_deref(),
            Some("+15423412398")
        );
    }

    #[test]
    fn owner_set_rejects_empty() {
        assert!(OwnerHandleSet::new(&[]).is_err());
        assert!(OwnerHandleSet::from_phones(&[]).is_err());
        assert!(OwnerHandleSet::from_phones(&["not-a-phone".into()]).is_err());
    }

    #[test]
    fn owner_handle_set_matches_typed_handles() {
        let owners = OwnerHandleSet::new(&[
            ("(555) 555-0100".into(), HandleType::Phone),
            ("Person@Example.COM".into(), HandleType::Email),
        ])
        .unwrap();
        assert!(owners.is_owner("+15555550100", HandleType::Phone));
        assert!(owners.is_owner("5555550100", HandleType::Phone));
        assert!(!owners.is_owner("5555550199", HandleType::Phone));
        assert!(owners.is_owner("person@example.com", HandleType::Email));
        assert!(!owners.is_owner("other@example.com", HandleType::Email));
        // A phone-shaped handle is not treated as an email or username handle.
        assert!(!owners.is_owner("(555) 555-0100", HandleType::Email));
        assert!(!owners.is_owner("Person@Example.COM", HandleType::Username));
    }

    #[test]
    #[allow(deprecated)]
    fn deprecated_owner_phone_set_still_works() {
        let owners = OwnerPhoneSet::new(&["(555) 555-0100".into()]).unwrap();
        assert_eq!(owners.primary_digits, "5555550100");
        assert!(owners.all_digits.contains("5555550100"));
        assert!(owners.is_owner("+15555550100"));
        assert!(!owners.is_owner("5555550199"));
        assert!(OwnerPhoneSet::new(&[]).is_err());
    }
}
