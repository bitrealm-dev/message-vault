//! Make contact names comparable for lookup.
//!
//! Trims whitespace, drops `__SUFFIX` markers, and lowercases so "Jordan_Alias"
//! and "jordan alias" hit the same map key.

/// Make a contact or export name comparable for map lookup.
///
/// Trailing `__SUFFIX` markers are dropped (for example `Jordan_Alias__SKIP`
/// becomes `jordan alias`). Underscores become spaces. Inner whitespace is
/// collapsed. The result is lowercased.
pub(crate) fn normalize_name_key(name: &str) -> String {
    let mut s = name.trim().to_string();
    if let Some(idx) = s.find("__") {
        s.truncate(idx);
    }
    s = s.replace('_', " ");
    collapse_inner_whitespace(&s).to_ascii_lowercase()
}

/// Collapse runs of whitespace into a single space and trim the ends.
pub(crate) fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True when a display name is missing or a known placeholder such as
/// `unknown` or `n/a`.
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
