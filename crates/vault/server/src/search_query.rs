//! Vault search query parser.
//!
//! Behavior reference: `web/src/lib/searchQuery.ts`.
//! Contract: both sides must match `fixtures/search/parse-cases.json`.
//! After changing the TypeScript grammar, run `node scripts/regen-search-goldens.mjs`
//! and update this module until Rust golden tests pass.

use chrono::{Datelike, Local};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateBounds {
    pub from: Option<String>,
    pub to: Option<String>,
}

impl DateBounds {
    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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

impl fmt::Display for SearchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationTypeFilter {
    Group,
    Individual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CountComparator {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
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

impl fmt::Display for CountComparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CountComparison {
    pub comparator: CountComparator,
    pub value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupBy {
    Conversation,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FtsNode {
    Term {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefix: Option<bool>,
    },
    Phrase {
        value: String,
    },
    And {
        children: Vec<FtsNode>,
    },
    Or {
        children: Vec<FtsNode>,
    },
    Not {
        child: Box<FtsNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedSearchQuery {
    pub mode: SearchMode,
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub exclude: Vec<String>,
    pub fts_ast: Option<FtsNode>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "with")]
    pub with_person: Option<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub has_attachment: Option<bool>,
    pub filename: Option<String>,
    pub filetype: Option<String>,
    pub larger_bytes: Option<u64>,
    pub smaller_bytes: Option<u64>,
    pub in_conversation: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub source: Option<String>,
    pub conversation_type: Option<ConversationTypeFilter>,
    pub group_by: GroupBy,
    pub context: u32,
    pub sort: SortOrder,
    pub within: Option<String>,
    pub handle: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub no_first_name: bool,
    pub no_last_name: bool,
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
            fts_ast: None,
            from: None,
            to: None,
            with_person: None,
            subject: None,
            text: None,
            has_attachment: None,
            filename: None,
            filetype: None,
            larger_bytes: None,
            smaller_bytes: None,
            in_conversation: None,
            after: None,
            before: None,
            source: None,
            conversation_type: None,
            group_by: GroupBy::Conversation,
            context: 0,
            sort: SortOrder::DateDesc,
            within: None,
            handle: None,
            first_name: None,
            last_name: None,
            phone: None,
            no_first_name: false,
            no_last_name: false,
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

        if bytes[i] == b'(' || bytes[i] == b')' {
            tokens.push((bytes[i] as char).to_string());
            i += 1;
            continue;
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
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'('
            && bytes[i] != b')'
        {
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

#[derive(Debug, Clone)]
enum FtsLex {
    Term { value: String, prefix: bool },
    Phrase { value: String },
    Or,
    And,
    Not,
    LParen,
    RParen,
}

fn append_fts_lexemes(token: &str, out: &mut Vec<FtsLex>) {
    if token == "(" {
        out.push(FtsLex::LParen);
        return;
    }
    if token == ")" {
        out.push(FtsLex::RParen);
        return;
    }
    let upper = token.to_ascii_uppercase();
    if upper == "OR" {
        out.push(FtsLex::Or);
        return;
    }
    if upper == "AND" {
        out.push(FtsLex::And);
        return;
    }
    if upper == "NOT" {
        out.push(FtsLex::Not);
        return;
    }

    let mut raw = token;
    if raw.starts_with('-') && raw.len() > 1 {
        out.push(FtsLex::Not);
        raw = &raw[1..];
    }

    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        let phrase = raw[1..raw.len() - 1].trim();
        if !phrase.is_empty() {
            out.push(FtsLex::Phrase {
                value: phrase.to_string(),
            });
        }
        return;
    }

    let mut prefix = false;
    let mut value = raw.to_string();
    if value.ends_with('*') && value.len() > 1 {
        prefix = true;
        value.pop();
    }
    if !value.is_empty() {
        out.push(FtsLex::Term { value, prefix });
    }
}

fn parse_fts_lexemes(lexemes: &[FtsLex]) -> Option<FtsNode> {
    let mut i = 0usize;

    fn peek(lexemes: &[FtsLex], i: usize) -> Option<&FtsLex> {
        lexemes.get(i)
    }

    fn parse_primary(lexemes: &[FtsLex], i: &mut usize) -> Option<FtsNode> {
        match peek(lexemes, *i)? {
            FtsLex::LParen => {
                *i += 1;
                let inner = parse_or(lexemes, i);
                if matches!(peek(lexemes, *i), Some(FtsLex::RParen)) {
                    *i += 1;
                }
                inner
            }
            FtsLex::Term { value, prefix } => {
                let node = FtsNode::Term {
                    value: value.clone(),
                    prefix: if *prefix { Some(true) } else { None },
                };
                *i += 1;
                Some(node)
            }
            FtsLex::Phrase { value } => {
                let node = FtsNode::Phrase {
                    value: value.clone(),
                };
                *i += 1;
                Some(node)
            }
            _ => None,
        }
    }

    fn parse_unary(lexemes: &[FtsLex], i: &mut usize) -> Option<FtsNode> {
        if matches!(peek(lexemes, *i), Some(FtsLex::Not)) {
            *i += 1;
            let child = parse_unary(lexemes, i)?;
            return Some(FtsNode::Not {
                child: Box::new(child),
            });
        }
        parse_primary(lexemes, i)
    }

    fn parse_and(lexemes: &[FtsLex], i: &mut usize) -> Option<FtsNode> {
        let mut nodes = Vec::new();
        let first = parse_unary(lexemes, i)?;
        nodes.push(first);
        loop {
            match peek(lexemes, *i) {
                None | Some(FtsLex::Or) | Some(FtsLex::RParen) => break,
                Some(FtsLex::And) => {
                    *i += 1;
                    let Some(next) = parse_unary(lexemes, i) else {
                        break;
                    };
                    nodes.push(next);
                }
                Some(
                    FtsLex::Not | FtsLex::LParen | FtsLex::Term { .. } | FtsLex::Phrase { .. },
                ) => {
                    let Some(next) = parse_unary(lexemes, i) else {
                        break;
                    };
                    nodes.push(next);
                }
            }
        }
        if nodes.len() == 1 {
            Some(nodes.remove(0))
        } else {
            Some(FtsNode::And { children: nodes })
        }
    }

    fn parse_or(lexemes: &[FtsLex], i: &mut usize) -> Option<FtsNode> {
        let mut nodes = Vec::new();
        let first = parse_and(lexemes, i)?;
        nodes.push(first);
        while matches!(peek(lexemes, *i), Some(FtsLex::Or)) {
            *i += 1;
            let Some(next) = parse_and(lexemes, i) else {
                break;
            };
            nodes.push(next);
        }
        if nodes.len() == 1 {
            Some(nodes.remove(0))
        } else {
            Some(FtsNode::Or { children: nodes })
        }
    }

    parse_or(lexemes, &mut i)
}

fn flatten_fts_leaves(
    node: &FtsNode,
    terms: &mut Vec<String>,
    phrases: &mut Vec<String>,
    exclude: &mut Vec<String>,
    negated: bool,
) {
    match node {
        FtsNode::Term { value, .. } => {
            if negated {
                exclude.push(value.clone());
            } else {
                terms.push(value.clone());
            }
        }
        FtsNode::Phrase { value } => {
            if negated {
                exclude.push(value.clone());
            } else {
                phrases.push(value.clone());
            }
        }
        FtsNode::Not { child } => {
            flatten_fts_leaves(child, terms, phrases, exclude, !negated);
        }
        FtsNode::And { children } | FtsNode::Or { children } => {
            for child in children {
                flatten_fts_leaves(child, terms, phrases, exclude, negated);
            }
        }
    }
}

fn normalize_date(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    // Relative Nd / Nw / Nm / Ny → local calendar day (match TypeScript).
    let bytes = t.as_bytes();
    if bytes.len() >= 2 {
        let unit = bytes[bytes.len() - 1].to_ascii_lowercase();
        if matches!(unit, b'd' | b'w' | b'm' | b'y') {
            let num = &t[..t.len() - 1];
            if !num.is_empty()
                && num.bytes().all(|b| b.is_ascii_digit())
                && let Ok(n) = num.parse::<i64>()
                && n >= 0
            {
                let today = Local::now().date_naive();
                let d = match unit {
                    b'd' => today
                        .checked_sub_signed(chrono::Duration::days(n))
                        .unwrap_or(today),
                    b'w' => today
                        .checked_sub_signed(chrono::Duration::days(n * 7))
                        .unwrap_or(today),
                    b'm' => {
                        let (y, m, day) = (today.year(), today.month(), today.day());
                        let total = i64::from(y) * 12 + i64::from(m) - 1 - n;
                        let ny = (total.div_euclid(12)) as i32;
                        let nm = (total.rem_euclid(12) + 1) as u32;
                        chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
                            .and_then(|_first| {
                                let last_day = if nm == 12 {
                                    chrono::NaiveDate::from_ymd_opt(ny + 1, 1, 1)
                                        .unwrap()
                                        .pred_opt()
                                        .unwrap()
                                        .day()
                                } else {
                                    chrono::NaiveDate::from_ymd_opt(ny, nm + 1, 1)
                                        .unwrap()
                                        .pred_opt()
                                        .unwrap()
                                        .day()
                                };
                                chrono::NaiveDate::from_ymd_opt(ny, nm, day.min(last_day))
                            })
                            .unwrap_or(today)
                    }
                    _ => {
                        let y = today.year() - n as i32;
                        chrono::NaiveDate::from_ymd_opt(y, today.month(), today.day())
                            .or_else(|| chrono::NaiveDate::from_ymd_opt(y, today.month(), 28))
                            .unwrap_or(today)
                    }
                };
                return Some(d.format("%Y-%m-%d").to_string());
            }
        }
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

pub(crate) fn parse_count_comparison(raw: &str) -> Option<CountComparison> {
    let t = raw.trim();
    let (comparator, digits) = if let Some(rest) = t.strip_prefix(">=") {
        (CountComparator::Gte, rest)
    } else if let Some(rest) = t.strip_prefix("<=") {
        (CountComparator::Lte, rest)
    } else if let Some(rest) = t.strip_prefix('>') {
        (CountComparator::Gt, rest)
    } else if let Some(rest) = t.strip_prefix('<') {
        (CountComparator::Lt, rest)
    } else {
        let rest = t.strip_prefix('=')?;
        (CountComparator::Eq, rest)
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(CountComparison {
        comparator,
        value: digits.parse().ok()?,
    })
}

fn parse_size_bytes(raw: &str) -> Option<u64> {
    let t = raw.trim().to_ascii_lowercase().replace(',', "");
    if t.is_empty() {
        return None;
    }
    let mut end_num = 0usize;
    let bytes = t.as_bytes();
    while end_num < bytes.len() && (bytes[end_num].is_ascii_digit() || bytes[end_num] == b'.') {
        end_num += 1;
    }
    if end_num == 0 {
        return None;
    }
    let n: f64 = t[..end_num].parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    let unit = t[end_num..].trim().trim_end_matches('b');
    let mult = match unit {
        "" => 1.0,
        "k" => 1024.0,
        "m" => 1024.0_f64.powi(2),
        "g" => 1024.0_f64.powi(3),
        "t" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((n * mult).round() as u64)
}

fn normalize_filetype(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    if v.is_empty() {
        return None;
    }
    if v == "pdf" {
        return Some("document".into());
    }
    Some(v)
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
        "search" | "with" | "from" | "to" | "subject" | "text" | "has" | "after" | "before"
        | "source" | "is" | "within" | "label" | "in" | "show" | "handle" | "filename"
        | "filetype" | "larger" | "smaller" | "group-count" | "message-count" | "group"
        | "context" | "sort" | "last-contact" | "first-contact" | "first" | "last" | "phone"
        | "conversation" => Some((op, value)),
        _ => None,
    }
}

/// Parse a vault search string into structured filters.
pub fn parse_search_query(input: &str) -> ParsedSearchQuery {
    let mut out = ParsedSearchQuery::default();
    if input.trim().is_empty() {
        return out;
    }

    let mut fts_lexemes = Vec::new();

    for raw in tokenize(input) {
        if let Some((op_raw, value_raw)) = parse_operator(&raw) {
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
                "to" => out.to = Some(value.to_string()),
                "with" => out.with_person = Some(value.to_string()),
                "subject" => out.subject = Some(value.to_string()),
                "text" => out.text = Some(value.to_string()),
                "has" => {
                    let v = value.to_ascii_lowercase();
                    if v == "attachment" || v == "att" {
                        out.has_attachment = Some(true);
                    } else if v == "noattachment" || v == "noatt" {
                        out.has_attachment = Some(false);
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
                    } else if v == "nofirst" {
                        out.no_first_name = true;
                    } else if v == "nolast" {
                        out.no_last_name = true;
                    } else if v == "nameless" {
                        out.no_first_name = true;
                        out.no_last_name = true;
                    }
                }
                "within" | "label" => out.within = Some(value.to_string()),
                "handle" => out.handle = Some(value.to_string()),
                "first" => out.first_name = Some(value.to_string()),
                "last" => out.last_name = Some(value.to_string()),
                "phone" => out.phone = Some(value.to_string()),
                "filename" => out.filename = Some(value.to_string()),
                "filetype" => out.filetype = normalize_filetype(value),
                "larger" => out.larger_bytes = parse_size_bytes(value),
                "smaller" => out.smaller_bytes = parse_size_bytes(value),
                "group" => {
                    let v = value.to_ascii_lowercase();
                    if matches!(v.as_str(), "none" | "messages" | "message") {
                        out.group_by = GroupBy::None;
                    } else if matches!(v.as_str(), "conversation" | "conversations") {
                        out.group_by = GroupBy::Conversation;
                    }
                }
                "context" => {
                    if let Ok(n) = value.parse::<u32>() {
                        out.context = n.min(20);
                    }
                }
                "sort" => {
                    let v = value.to_ascii_lowercase();
                    if matches!(v.as_str(), "date-asc" | "oldest" | "asc") {
                        out.sort = SortOrder::DateAsc;
                    } else if matches!(v.as_str(), "relevance" | "rank" | "best") {
                        out.sort = SortOrder::Relevance;
                    } else if matches!(v.as_str(), "date" | "date-desc" | "newest" | "desc") {
                        out.sort = SortOrder::DateDesc;
                    }
                }
                "in" => {
                    if !value.eq_ignore_ascii_case("trash") {
                        out.in_conversation = Some(value.to_string());
                    }
                }
                "conversation" => out.in_conversation = Some(value.to_string()),
                "show" => {}
                "last-contact" => out.last_contact = parse_date_bounds(value),
                "first-contact" => out.first_contact = parse_date_bounds(value),
                "group-count" => out.group_count = parse_count_comparison(value),
                "message-count" => out.message_count = parse_count_comparison(value),
                _ => {}
            }
            continue;
        }

        append_fts_lexemes(&raw, &mut fts_lexemes);
    }

    out.fts_ast = parse_fts_lexemes(&fts_lexemes);
    if let Some(ast) = &out.fts_ast {
        flatten_fts_leaves(
            ast,
            &mut out.terms,
            &mut out.phrases,
            &mut out.exclude,
            false,
        );
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
    !q.terms.is_empty() || !q.phrases.is_empty() || !q.exclude.is_empty() || q.fts_ast.is_some()
}

pub fn has_search_criteria(q: &ParsedSearchQuery) -> bool {
    q.fts_ast.is_some()
        || has_metadata_text_criteria(q)
        || q.mode == SearchMode::Contacts
        || q.from.is_some()
        || q.to.is_some()
        || q.with_person.is_some()
        || q.subject.is_some()
        || q.text.is_some()
        || q.has_attachment.is_some()
        || q.filename.is_some()
        || q.filetype.is_some()
        || q.larger_bytes.is_some()
        || q.smaller_bytes.is_some()
        || q.in_conversation.is_some()
        || q.after.is_some()
        || q.before.is_some()
        || q.source.is_some()
        || q.conversation_type.is_some()
        || q.within.is_some()
        || q.handle.is_some()
        || q.first_name.is_some()
        || q.last_name.is_some()
        || q.phone.is_some()
        || q.no_first_name
        || q.no_last_name
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
    use serde_json::Value;

    #[test]
    fn golden_parse_cases_match_typescript() {
        let raw = include_str!("../../../../fixtures/search/parse-cases.json");
        let cases: Value = serde_json::from_str(raw).unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let input = case["input"].as_str().unwrap();
            let expected = case.get("expected").expect("golden expected missing");
            let parsed = parse_search_query(input);
            let actual = serde_json::to_value(&parsed).unwrap();
            assert_eq!(
                actual,
                *expected,
                "golden mismatch for case {name:?}\ninput: {input:?}\nactual: {}\nexpected: {}",
                serde_json::to_string_pretty(&actual).unwrap(),
                serde_json::to_string_pretty(expected).unwrap()
            );
        }
    }

    #[test]
    fn relative_after_produces_ymd() {
        let q = parse_search_query("after:7d");
        let after = q.after.expect("after");
        assert!(
            after.len() == 10 && after.as_bytes().get(4) == Some(&b'-'),
            "unexpected after: {after}"
        );
    }

    #[test]
    fn show_contact_is_not_criteria() {
        assert!(!has_search_criteria(&parse_search_query("show:contact")));
    }
}
