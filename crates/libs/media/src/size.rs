use anyhow::{Result, bail};

/// Parse sizes like `20M`, `512k`, `100` (bytes).
pub(crate) fn parse_size(raw: &str) -> Result<u64> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty size");
    }
    let (num, mult) = match s.as_bytes().last().copied() {
        Some(b) if b.eq_ignore_ascii_case(&b'k') => (&s[..s.len() - 1], 1024u64),
        Some(b) if b.eq_ignore_ascii_case(&b'm') => (&s[..s.len() - 1], 1024 * 1024),
        Some(b) if b.eq_ignore_ascii_case(&b'g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s, 1u64),
    };
    let n: u64 = num
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid size: {raw}"))?;
    Ok(n.saturating_mul(mult))
}
