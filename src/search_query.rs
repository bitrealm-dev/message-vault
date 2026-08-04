//! Fastmail-style vault search query parser (parity with web `searchQuery.ts`).
//!
//! Supported operators:
//!   search:contacts  handle:  within:  last-contact:  first-contact:
//!   group-count:  message-count:
//!   with:  from:  to:  has:attachment  after:  before:  source:
//!   is:group  is:direct
//!   "quoted phrases"  -term

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DateBounds {
    pub from: Option<String>,
    pub to: Option<String>,
}

impl DateBounds {
    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Messages,
    Contacts,
}

impl SearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Contacts => "contacts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationTypeFilter {
    Group,
    Individual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountComparator {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl CountComparator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountComparison {
    pub comparator: CountComparator,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSearchQuery {
    pub mode: SearchMode,
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub exclude: Vec<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub subject: Option<String>,
    pub has_attachment: bool,
    pub after: Option<String>,
    pub before: Option<String>,
    pub source: Option<String>,
    pub conversation_type: Option<ConversationTypeFilter>,
    pub within: Option<String>,
    pub handle: Option<String>,
    pub last_contact: DateBounds,
    pub first_contact: DateBounds,
    pub group_count: Option<CountComparison>,
    pub message_count: Option<CountComparison>,
    /// Retired legacy presentation operator; always false.
    pub show_contact: bool,
}

impl Default for ParsedSearchQuery {
    fn default() -> Self {
        Self {
            mode: SearchMode::Messages,
            terms: Vec::new(),
            phrases: Vec::new(),
            exclude: Vec::new(),
            from: None,
            to: None,
            subject: None,
            has_attachment: false,
            after: None,
            before: None,
            source: None,
            conversation_type: None,
            within: None,
            handle: None,
            last_contact: DateBounds::default(),
            first_contact: DateBounds::default(),
            group_count: None,
            message_count: None,
            show_contact: false,
        }
    }
}

fn read_quoted(s: &str, start: usize) -> (String, usize) {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut phrase = String::new();
    while i < bytes.len() && bytes[i] != b'"' {
        phrase.push(bytes[i] as char);
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'"' {
        i += 1;
    }
    (phrase, i)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let s = input.trim();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
            let (value, next) = read_quoted(s, i + 2);
            tokens.push(format!("-\"{value}\""));
            i = next;
            continue;
        }

        if bytes[i] == b'"' {
            let (value, next) = read_quoted(s, i + 1);
            tokens.push(format!("\"{value}\""));
            i = next;
            continue;
        }

        let mut tok = String::new();
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            if bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                tok.push_str(":\"");
                let (value, next) = read_quoted(s, i + 2);
                tok.push_str(&value);
                tok.push('"');
                i = next;
                break;
            }
            tok.push(bytes[i] as char);
            i += 1;
        }
        if !tok.is_empty() {
            tokens.push(tok);
        }
    }
    tokens
}

fn normalize_date(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if t.len() == 10
        && t.as_bytes().get(4) == Some(&b'-')
        && t.as_bytes().get(7) == Some(&b'-')
        && t.bytes().all(|b| b.is_ascii_digit() || b == b'-')
    {
        return Some(t.to_string());
    }
    if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
        return Some(format!("{t}-01-01"));
    }
    Some(t.to_string())
}

fn parse_date_bounds(raw: &str) -> DateBounds {
    let t = raw.trim();
    if t.is_empty() {
        return DateBounds::default();
    }
    if let Some((a, b)) = t.split_once("..") {
        return DateBounds {
            from: normalize_date(a),
            to: normalize_date(b),
        };
    }
    if let Some(rest) = t.strip_prefix(">=") {
        return DateBounds {
            from: normalize_date(rest),
            to: None,
        };
    }
    if let Some(rest) = t.strip_prefix('>') {
        return DateBounds {
            from: normalize_date(rest),
            to: None,
        };
    }
    if let Some(rest) = t.strip_prefix("<=") {
        return DateBounds {
            from: None,
            to: normalize_date(rest),
        };
    }
    if let Some(rest) = t.strip_prefix('<') {
        return DateBounds {
            from: None,
            to: normalize_date(rest),
        };
    }
    DateBounds {
        from: None,
        to: normalize_date(t),
    }
}

fn parse_count_comparison(raw: &str) -> Option<CountComparison> {
    let t = raw.trim();
    let (comparator, digits) = if let Some(rest) = t.strip_prefix(">=") {
        (CountComparator::Gte, rest)
    } else if let Some(rest) = t.strip_prefix("<=") {
        (CountComparator::Lte, rest)
    } else if let Some(rest) = t.strip_prefix('>') {
        (CountComparator::Gt, rest)
    } else if let Some(rest) = t.strip_prefix('<') {
        (CountComparator::Lt, rest)
    } else if let Some(rest) = t.strip_prefix('=') {
        (CountComparator::Eq, rest)
    } else {
        return None;
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(CountComparison {
        comparator,
        value: digits.parse().ok()?,
    })
}

fn strip_surrounding_quotes(value: &str) -> &str {
    let t = value.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

fn parse_operator(token: &str) -> Option<(&str, &str)> {
    let (op, value) = token.split_once(':')?;
    let op_l = op.to_ascii_lowercase();
    match op_l.as_str() {
        "search" | "with" | "from" | "to" | "subject" | "has" | "after" | "before" | "source"
        | "is" | "within" | "label" | "in" | "show" | "handle" | "last-contact"
        | "first-contact" | "group-count" | "message-count" => {
            // Keep owned lowercase op via leak-free approach: return original slice bounds
            // by matching case-insensitively on the prefix length.
            Some((op, value))
        }
        _ => None,
    }
}

/// Parse a vault search string into structured filters.
pub fn parse_search_query(input: &str) -> ParsedSearchQuery {
    let mut out = ParsedSearchQuery::default();
    if input.trim().is_empty() {
        return out;
    }

    for raw in tokenize(input) {
        let mut token = raw.as_str();
        let mut negated = false;
        if token.starts_with('-') && token.len() > 1 {
            negated = true;
            token = &token[1..];
        }

        if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
            let phrase = token[1..token.len() - 1].trim();
            if phrase.is_empty() {
                continue;
            }
            if negated {
                out.exclude.push(phrase.to_string());
            } else {
                out.phrases.push(phrase.to_string());
            }
            continue;
        }

        if let Some((op_raw, value_raw)) = parse_operator(token) {
            let op = op_raw.to_ascii_lowercase();
            let value = strip_surrounding_quotes(value_raw).trim();
            if value.is_empty() && op != "has" {
                continue;
            }
            match op.as_str() {
                "search" => {
                    let mode = value.to_ascii_lowercase();
                    if mode == "contacts" {
                        out.mode = SearchMode::Contacts;
                    } else if mode == "messages" {
                        out.mode = SearchMode::Messages;
                    }
                }
                "from" => out.from = Some(value.to_string()),
                "with" | "to" => out.to = Some(value.to_string()),
                "subject" => out.subject = Some(value.to_string()),
                "has" => {
                    if value.eq_ignore_ascii_case("attachment") {
                        out.has_attachment = true;
                    }
                }
                "after" => out.after = normalize_date(value),
                "before" => out.before = normalize_date(value),
                "source" => out.source = Some(value.to_string()),
                "is" => {
                    let v = value.to_ascii_lowercase();
                    if v == "group" {
                        out.conversation_type = Some(ConversationTypeFilter::Group);
                    } else if matches!(v.as_str(), "direct" | "individual" | "1-1") {
                        out.conversation_type = Some(ConversationTypeFilter::Individual);
                    }
                }
                "within" | "label" => out.within = Some(value.to_string()),
                "handle" => out.handle = Some(value.to_string()),
                "in" | "show" => {}
                "last-contact" => out.last_contact = parse_date_bounds(value),
                "first-contact" => out.first_contact = parse_date_bounds(value),
                "group-count" => out.group_count = parse_count_comparison(value),
                "message-count" => out.message_count = parse_count_comparison(value),
                _ => {}
            }
            continue;
        }

        if negated {
            out.exclude.push(token.to_string());
        } else {
            out.terms.push(token.to_string());
        }
    }

    out
}

pub fn has_date_bounds(bounds: &DateBounds) -> bool {
    !bounds.is_empty()
}

pub fn metadata_include_terms(q: &ParsedSearchQuery) -> Vec<&str> {
    q.terms
        .iter()
        .chain(q.phrases.iter())
        .map(String::as_str)
        .collect()
}

pub fn metadata_exclude_terms(q: &ParsedSearchQuery) -> Vec<&str> {
    q.exclude.iter().map(String::as_str).collect()
}

pub fn has_metadata_text_criteria(q: &ParsedSearchQuery) -> bool {
    !q.terms.is_empty() || !q.phrases.is_empty() || !q.exclude.is_empty()
}

pub fn has_search_criteria(q: &ParsedSearchQuery) -> bool {
    has_metadata_text_criteria(q)
        || q.mode == SearchMode::Contacts
        || q.from.is_some()
        || q.to.is_some()
        || q.subject.is_some()
        || q.has_attachment
        || q.after.is_some()
        || q.before.is_some()
        || q.source.is_some()
        || q.conversation_type.is_some()
        || q.within.is_some()
        || q.handle.is_some()
        || q.group_count.is_some()
        || q.message_count.is_some()
        || has_date_bounds(&q.last_contact)
        || has_date_bounds(&q.first_contact)
}

impl fmt::Display for ConversationTypeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group => write!(f, "group"),
            Self::Individual => write!(f, "individual"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_free_text_terms_and_phrases() {
        let q = parse_search_query("hello \"exact phrase\" world");
        assert_eq!(q.terms, ["hello", "world"]);
        assert_eq!(q.phrases, ["exact phrase"]);
    }

    #[test]
    fn parses_operators() {
        let q = parse_search_query(
            "from:alice with:bob has:attachment after:2020-01-01 before:2021 source:imessage is:group",
        );
        assert_eq!(q.from.as_deref(), Some("alice"));
        assert_eq!(q.to.as_deref(), Some("bob"));
        assert!(q.has_attachment);
        assert_eq!(q.after.as_deref(), Some("2020-01-01"));
        assert_eq!(q.before.as_deref(), Some("2021-01-01"));
        assert_eq!(q.source.as_deref(), Some("imessage"));
        assert_eq!(q.conversation_type, Some(ConversationTypeFilter::Group));
        assert_eq!(q.mode, SearchMode::Messages);
    }

    #[test]
    fn label_alias_and_in_trash_ignored() {
        assert_eq!(parse_search_query("label:Work").within.as_deref(), Some("Work"));
        let q = parse_search_query("in:trash hello");
        assert_eq!(q.terms, ["hello"]);
    }

    #[test]
    fn with_and_to_same() {
        assert_eq!(parse_search_query("with:sam").to.as_deref(), Some("sam"));
        assert_eq!(parse_search_query("to:sam").to.as_deref(), Some("sam"));
    }

    #[test]
    fn parses_negation() {
        let q = parse_search_query("party -cake -\"bad word\"");
        assert_eq!(q.terms, ["party"]);
        assert_eq!(q.exclude, ["cake", "bad word"]);
    }

    #[test]
    fn is_direct_is_individual() {
        assert_eq!(
            parse_search_query("is:direct").conversation_type,
            Some(ConversationTypeFilter::Individual)
        );
    }

    #[test]
    fn first_last_contact_bounds() {
        assert_eq!(
            parse_search_query("first-contact:>=2020-01-01").first_contact,
            DateBounds {
                from: Some("2020-01-01".into()),
                to: None,
            }
        );
        assert_eq!(
            parse_search_query("first-contact:<2020-01-01").first_contact,
            DateBounds {
                from: None,
                to: Some("2020-01-01".into()),
            }
        );
        assert_eq!(
            parse_search_query("first-contact:2020-01-01..2020-06-30").first_contact,
            DateBounds {
                from: Some("2020-01-01".into()),
                to: Some("2020-06-30".into()),
            }
        );
        assert_eq!(
            parse_search_query("last-contact:2024-01-15").last_contact,
            DateBounds {
                from: None,
                to: Some("2024-01-15".into()),
            }
        );
        assert_eq!(
            parse_search_query("first-contact:2015").first_contact,
            DateBounds {
                from: None,
                to: Some("2015-01-01".into()),
            }
        );
        assert!(has_search_criteria(&parse_search_query(
            "last-contact:2024-01-01"
        )));
    }

    #[test]
    fn defaults_messages_and_retires_show_contact() {
        assert_eq!(parse_search_query("hello").mode, SearchMode::Messages);
        assert!(!parse_search_query("show:contact").show_contact);
        assert!(!has_search_criteria(&parse_search_query("show:contact")));
    }

    #[test]
    fn parses_contact_mode() {
        let q = parse_search_query(
            "search:contacts within:\"Close Friends\" handle:\"Ann Lee\" group-count:>=2 message-count:<100",
        );
        assert_eq!(q.mode, SearchMode::Contacts);
        assert_eq!(q.within.as_deref(), Some("Close Friends"));
        assert_eq!(q.handle.as_deref(), Some("Ann Lee"));
        assert_eq!(
            q.group_count,
            Some(CountComparison {
                comparator: CountComparator::Gte,
                value: 2
            })
        );
        assert_eq!(
            q.message_count,
            Some(CountComparison {
                comparator: CountComparator::Lt,
                value: 100
            })
        );
        assert!(has_search_criteria(&q));
    }

    #[test]
    fn ignores_invalid_count_comparisons() {
        assert_eq!(parse_search_query("group-count:2").group_count, None);
        assert_eq!(parse_search_query("message-count:>=-1").message_count, None);
        assert_eq!(parse_search_query("message-count:>1.5").message_count, None);
    }
}
