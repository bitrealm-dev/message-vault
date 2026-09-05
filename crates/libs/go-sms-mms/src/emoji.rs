//! Decode Go SMS Pro emoji codes like `+g1f602` into Unicode.

use regex::Regex;
use std::sync::LazyLock;

static EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+g([0-9a-fA-F]+)").expect("emoji regex"));

/// Replace Go SMS Pro `+g` hex codes with the matching Unicode characters.
///
/// Codes that are not valid Unicode stay unchanged. For example, `+g1f602`
/// becomes 😂.
pub fn decode_gosms_emojis(text: &str) -> String {
    EMOJI_RE
        .replace_all(text, |caps: &regex::Captures| {
            hex_code_to_char(&caps[1]).unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}

/// Parse a hex code point into a one-character string.
fn hex_code_to_char(hex: &str) -> Option<String> {
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .map(|c| c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_emoji_code() {
        assert_eq!(decode_gosms_emojis("hi +g1f602"), "hi 😂");
    }
}
