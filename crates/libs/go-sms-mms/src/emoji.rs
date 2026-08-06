//! Decode Go SMS Pro emoji codes like `+g1f602` into Unicode.

use regex::Regex;
use std::sync::OnceLock;

static EMOJI_RE: OnceLock<Regex> = OnceLock::new();

pub fn decode_gosms_emojis(text: &str) -> String {
    let re = EMOJI_RE.get_or_init(|| Regex::new(r"\+g([0-9a-fA-F]+)").expect("emoji regex"));
    re.replace_all(text, |caps: &regex::Captures| {
        u32::from_str_radix(&caps[1], 16)
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_emoji_code() {
        assert_eq!(decode_gosms_emojis("hi +g1f602"), "hi 😂");
    }
}
