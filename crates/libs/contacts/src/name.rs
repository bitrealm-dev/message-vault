//! Name key normalization for contact lookup.

/// Normalize a contact / export name for map lookup.
pub(crate) fn normalize_name_key(name: &str) -> String {
    let mut s = name.trim().to_string();
    // Strip trailing __SUFFIX markers (e.g. Jordan_Alias__SKIP).
    if let Some(idx) = s.find("__") {
        s.truncate(idx);
    }
    s = s.replace('_', " ");
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(crate) fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True when a display name is missing or a known placeholder.
pub(crate) fn is_blank_or_unknown_name(name: &str) -> bool {
    let t = name.trim();
    if t.is_empty() {
        return true;
    }
    matches!(
        t.to_ascii_lowercase().as_str(),
        "unknown" | "null" | "(unknown)" | "n/a" | "na"
    )
}
