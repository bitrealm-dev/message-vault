//! Vault search query parser.
//!
//! Expected parse results live in `tests/fixtures/search/parse-cases.json`.
//! The deprecated TypeScript parser in `web-next/src/lib/searchQuery.ts` used
//! to share that contract. After changing the TypeScript grammar, run
//! `node scripts/deprecated/regen-search-goldens.mjs` and update this module
//! until the Rust tests pass. Once web-next is gone, this parser stands alone
//! and those JSON cases become a plain Rust regression check.

use chrono::{Datelike, Local};
use serde::Serialize;
use std::fmt;

use crate::export_api::ExportQueryError;

/// Reject huge search strings before parsing or SQL construction.
pub const MAX_SEARCH_QUERY_BYTES: usize = 2_048;
pub const MAX_SEARCH_TEXT_TERMS: usize = 32;
/// Hard cap on full-text search expression nodes (guards nested OR/AND abuse).
pub const MAX_FTS_NODES: usize = 64;
/// Hard cap on parenthesis / negation nesting depth.
///
/// The node cap can only be applied to a tree that already parsed, and
/// parentheses do not add nodes, so the recursive-descent parser needs its own
/// limit to keep a query like `((((…alpha…))))` from exhausting the stack.
pub const MAX_FTS_DEPTH: usize = 32;
/// Relative `after:`/`before:` windows larger than this many days are rejected.
pub const MAX_RELATIVE_LOOKBACK_DAYS: i64 = 3_650;

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
    /// Hide threads that involve this contact group (`-people:`).
    pub exclude_people: Option<String>,
    /// Thread tag include (`tag:`).
    pub tag: Option<String>,
    /// Hide threads that have this tag (`-tag:`).
    pub exclude_tag: Option<String>,
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
            exclude_people: None,
            tag: None,
            exclude_tag: None,
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

/// Read characters until the next `"` and return `(phrase, index_after_quote)`.
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

/// Split a search string into operator tokens, quoted phrases, and parentheses.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsParseError {
    IncompleteOperand,
    UnmatchedOpeningParenthesis,
    UnmatchedClosingParenthesis,
    UnconsumedTokens,
    TooDeeplyNested,
}

impl fmt::Display for FtsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteOperand => f.write_str("boolean operator is missing an operand"),
            Self::UnmatchedOpeningParenthesis => f.write_str("unmatched opening parenthesis"),
            Self::UnmatchedClosingParenthesis => f.write_str("unmatched closing parenthesis"),
            Self::UnconsumedTokens => f.write_str("unconsumed boolean expression tokens"),
            Self::TooDeeplyNested => f.write_fmt(format_args!(
                "expression nests deeper than {MAX_FTS_DEPTH} levels"
            )),
        }
    }
}

impl std::error::Error for FtsParseError {}

fn parse_fts_lexemes(lexemes: &[FtsLex]) -> Result<Option<FtsNode>, FtsParseError> {
    if lexemes.is_empty() {
        return Ok(None);
    }
    let mut i = 0usize;

    fn peek(lexemes: &[FtsLex], i: usize) -> Option<&FtsLex> {
        lexemes.get(i)
    }

    fn parse_primary(
        lexemes: &[FtsLex],
        i: &mut usize,
        depth: usize,
    ) -> Result<FtsNode, FtsParseError> {
        match peek(lexemes, *i) {
            Some(FtsLex::LParen) => {
                if depth >= MAX_FTS_DEPTH {
                    return Err(FtsParseError::TooDeeplyNested);
                }
                *i += 1;
                let inner = parse_or(lexemes, i, depth + 1)?;
                if !matches!(peek(lexemes, *i), Some(FtsLex::RParen)) {
                    return Err(FtsParseError::UnmatchedOpeningParenthesis);
                }
                *i += 1;
                Ok(inner)
            }
            Some(FtsLex::Term { value, prefix }) => {
                let node = FtsNode::Term {
                    value: value.clone(),
                    prefix: if *prefix { Some(true) } else { None },
                };
                *i += 1;
                Ok(node)
            }
            Some(FtsLex::Phrase { value }) => {
                let node = FtsNode::Phrase {
                    value: value.clone(),
                };
                *i += 1;
                Ok(node)
            }
            Some(FtsLex::RParen) => Err(FtsParseError::UnmatchedClosingParenthesis),
            Some(FtsLex::Or | FtsLex::And | FtsLex::Not) | None => {
                Err(FtsParseError::IncompleteOperand)
            }
        }
    }

    fn parse_unary(
        lexemes: &[FtsLex],
        i: &mut usize,
        depth: usize,
    ) -> Result<FtsNode, FtsParseError> {
        if matches!(peek(lexemes, *i), Some(FtsLex::Not)) {
            if depth >= MAX_FTS_DEPTH {
                return Err(FtsParseError::TooDeeplyNested);
            }
            *i += 1;
            let child = parse_unary(lexemes, i, depth + 1)?;
            return Ok(FtsNode::Not {
                child: Box::new(child),
            });
        }
        parse_primary(lexemes, i, depth)
    }

    fn parse_and(
        lexemes: &[FtsLex],
        i: &mut usize,
        depth: usize,
    ) -> Result<FtsNode, FtsParseError> {
        let mut nodes = Vec::new();
        let first = parse_unary(lexemes, i, depth)?;
        nodes.push(first);
        loop {
            match peek(lexemes, *i) {
                None | Some(FtsLex::Or) | Some(FtsLex::RParen) => break,
                Some(FtsLex::And) => {
                    *i += 1;
                    nodes.push(parse_unary(lexemes, i, depth)?);
                }
                Some(
                    FtsLex::Not | FtsLex::LParen | FtsLex::Term { .. } | FtsLex::Phrase { .. },
                ) => {
                    nodes.push(parse_unary(lexemes, i, depth)?);
                }
            }
        }
        if nodes.len() == 1 {
            Ok(nodes.remove(0))
        } else {
            Ok(FtsNode::And { children: nodes })
        }
    }

    fn parse_or(lexemes: &[FtsLex], i: &mut usize, depth: usize) -> Result<FtsNode, FtsParseError> {
        let mut nodes = Vec::new();
        let first = parse_and(lexemes, i, depth)?;
        nodes.push(first);
        while matches!(peek(lexemes, *i), Some(FtsLex::Or)) {
            *i += 1;
            nodes.push(parse_and(lexemes, i, depth)?);
        }
        if nodes.len() == 1 {
            Ok(nodes.remove(0))
        } else {
            Ok(FtsNode::Or { children: nodes })
        }
    }

    let node = parse_or(lexemes, &mut i, 0)?;
    if i != lexemes.len() {
        return Err(if matches!(peek(lexemes, i), Some(FtsLex::RParen)) {
            FtsParseError::UnmatchedClosingParenthesis
        } else {
            FtsParseError::UnconsumedTokens
        });
    }
    Ok(Some(node))
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

/// Count nodes in a full-text search expression tree (to reject huge queries).
pub fn count_fts_nodes(node: &FtsNode) -> usize {
    match node {
        FtsNode::Term { .. } | FtsNode::Phrase { .. } => 1,
        FtsNode::Not { child } => 1 + count_fts_nodes(child),
        FtsNode::And { children } | FtsNode::Or { children } => {
            let child_nodes: usize = children.iter().map(count_fts_nodes).sum();
            1 + child_nodes
        }
    }
}

fn validate_search_query_bytes(input: &str) -> Result<(), ExportQueryError> {
    if input.len() > MAX_SEARCH_QUERY_BYTES {
        return Err(ExportQueryError::bad(format!(
            "search query exceeds {MAX_SEARCH_QUERY_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_search_query_complexity(
    text_term_count: usize,
    node_count: usize,
) -> Result<(), ExportQueryError> {
    if text_term_count > MAX_SEARCH_TEXT_TERMS {
        return Err(ExportQueryError::bad(format!(
            "search query has too many text terms (max {MAX_SEARCH_TEXT_TERMS})"
        )));
    }
    if node_count > MAX_FTS_NODES {
        return Err(ExportQueryError::bad(format!(
            "search query is too complex (max {MAX_FTS_NODES} expression nodes)"
        )));
    }
    Ok(())
}

/// Enforce size limits without treating list-search text as boolean syntax.
///
/// # Errors
///
/// Returns a bad-request error when the query is too long or has too many terms.
pub fn validate_list_search_query(input: &str) -> Result<(), ExportQueryError> {
    validate_search_query_bytes(input)?;
    let tokens = tokenize(input);
    let mut text_term_count = 0usize;
    for token in &tokens {
        if token.as_str() != "(" && token.as_str() != ")" {
            text_term_count += 1;
        }
    }
    validate_search_query_complexity(text_term_count, tokens.len())
}

/// Parse a boolean search query and enforce size limits.
///
/// # Errors
///
/// Returns a bad-request error when the query is too long, too complex, or
/// not valid boolean syntax.
pub fn validate_search_query(input: &str) -> Result<ParsedSearchQuery, ExportQueryError> {
    validate_search_query_bytes(input)?;
    let parsed = parse_search_query(input)
        .map_err(|error| ExportQueryError::bad(format!("invalid search expression: {error}")))?;
    let text_term_count = parsed.terms.len() + parsed.phrases.len() + parsed.exclude.len();
    let node_count = match &parsed.fts_ast {
        Some(ast) => count_fts_nodes(ast),
        None => 0,
    };
    validate_search_query_complexity(text_term_count, node_count)?;
    Ok(parsed)
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
                let lookback_days = match unit {
                    b'd' => n,
                    b'w' => n.saturating_mul(7),
                    b'm' => n.saturating_mul(31),
                    b'y' => n.saturating_mul(365),
                    _ => n,
                };
                if lookback_days > MAX_RELATIVE_LOOKBACK_DAYS {
                    return None;
                }
                let today = Local::now().date_naive();
                let d = match unit {
                    b'd' => today
                        .checked_sub_signed(chrono::Duration::days(n))
                        .unwrap_or(today),
                    b'w' => today
                        .checked_sub_signed(chrono::Duration::days(n * 7))
                        .unwrap_or(today),
                    b'm' => shift_calendar_months(today, n),
                    _ => shift_calendar_years(today, n),
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

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_of_next = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    Some(first_of_next.pred_opt()?.day())
}

fn shift_calendar_months(today: chrono::NaiveDate, months: i64) -> chrono::NaiveDate {
    let (y, m, day) = (today.year(), today.month(), today.day());
    let total = i64::from(y) * 12 + i64::from(m) - 1 - months;
    let year = (total.div_euclid(12)) as i32;
    let month = (total.rem_euclid(12) + 1) as u32;
    let last_day = last_day_of_month(year, month).unwrap_or(day);
    chrono::NaiveDate::from_ymd_opt(year, month, day.min(last_day)).unwrap_or(today)
}

fn shift_calendar_years(today: chrono::NaiveDate, years: i64) -> chrono::NaiveDate {
    let year = today.year() - years as i32;
    chrono::NaiveDate::from_ymd_opt(year, today.month(), today.day())
        .or_else(|| chrono::NaiveDate::from_ymd_opt(year, today.month(), 28))
        .unwrap_or(today)
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
        | "source" | "is" | "within" | "label" | "people" | "tag" | "in" | "show" | "handle"
        | "filename"
        | "filetype" | "larger" | "smaller" | "group-count" | "message-count" | "group"
        | "context" | "sort" | "last-contact" | "first-contact" | "first" | "last" | "phone"
        | "conversation" => Some((op, value)),
        _ => None,
    }
}

/// Parse a vault search string into structured filters.
///
/// # Errors
///
/// Returns an error when boolean operators or parentheses are unbalanced.
pub fn parse_search_query(input: &str) -> Result<ParsedSearchQuery, FtsParseError> {
    let mut out = ParsedSearchQuery::default();
    if input.trim().is_empty() {
        return Ok(out);
    }

    let mut fts_lexemes = Vec::new();

    for raw in tokenize(input) {
        let (negated, token) = if let Some(rest) = raw.strip_prefix('-') {
            if rest.contains(':') {
                (true, rest)
            } else {
                (false, raw.as_str())
            }
        } else {
            (false, raw.as_str())
        };
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
                "within" | "label" | "people" => {
                    if negated {
                        out.exclude_people = Some(value.to_string());
                    } else {
                        out.within = Some(value.to_string());
                    }
                }
                "tag" => {
                    if negated {
                        out.exclude_tag = Some(value.to_string());
                    } else {
                        out.tag = Some(value.to_string());
                    }
                }
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

    out.fts_ast = parse_fts_lexemes(&fts_lexemes)?;
    if let Some(ast) = &out.fts_ast {
        flatten_fts_leaves(
            ast,
            &mut out.terms,
            &mut out.phrases,
            &mut out.exclude,
            false,
        );
    }
    Ok(out)
}

#[cfg(test)]
pub fn has_metadata_text_criteria(q: &ParsedSearchQuery) -> bool {
    !q.terms.is_empty() || !q.phrases.is_empty() || !q.exclude.is_empty() || q.fts_ast.is_some()
}

#[cfg(test)]
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
        || !q.last_contact.is_empty()
        || !q.first_contact.is_empty()
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
        let raw = include_str!("../../../../tests/fixtures/search/parse-cases.json");
        let cases: Value = serde_json::from_str(raw).unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let input = case["input"].as_str().unwrap();
            let expected = case.get("expected").expect("golden expected missing");
            let parsed = parse_search_query(input).unwrap();
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
        let q = parse_search_query("after:7d").unwrap();
        let after = q.after.expect("after");
        assert!(
            after.len() == 10 && after.as_bytes().get(4) == Some(&b'-'),
            "unexpected after: {after}"
        );
    }

    #[test]
    fn relative_after_rejects_extreme_lookback() {
        let q = parse_search_query("after:99999d").unwrap();
        assert!(q.after.is_none(), "extreme relative date should be dropped");
    }

    #[test]
    fn show_contact_is_not_criteria() {
        assert!(!has_search_criteria(
            &parse_search_query("show:contact").unwrap()
        ));
    }

    #[test]
    fn malformed_boolean_queries_are_rejected() {
        for query in [
            "foo OR",
            "foo AND",
            "NOT",
            "(foo OR bar",
            "foo OR bar)",
            "foo ) bar",
        ] {
            let error = validate_search_query(query).unwrap_err();
            assert!(
                matches!(error, ExportQueryError::BadRequest(_)),
                "query should be rejected: {query}"
            );
        }
    }

    #[test]
    fn deeply_nested_boolean_query_is_rejected() {
        let depth = MAX_FTS_DEPTH + 8;
        let nested = format!("{}alpha{}", "(".repeat(depth), ")".repeat(depth));
        let negations = format!("{}alpha", "NOT ".repeat(depth));
        for query in [nested, negations] {
            let error = validate_search_query(&query).unwrap_err();
            assert!(
                matches!(error, ExportQueryError::BadRequest(_)),
                "deeply nested query should be rejected: {query}"
            );
        }
    }

    #[test]
    fn nesting_within_the_depth_limit_still_parses() {
        let depth = 8;
        let query = format!("{}alpha OR beta{}", "(".repeat(depth), ")".repeat(depth));
        let parsed = validate_search_query(&query).unwrap();
        assert_eq!(parsed.terms, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn boolean_parser_preserves_nested_precedence() {
        let parsed = parse_search_query("foo OR (bar AND baz)").unwrap();
        assert_eq!(
            parsed.fts_ast,
            Some(FtsNode::Or {
                children: vec![
                    FtsNode::Term {
                        value: "foo".into(),
                        prefix: None,
                    },
                    FtsNode::And {
                        children: vec![
                            FtsNode::Term {
                                value: "bar".into(),
                                prefix: None,
                            },
                            FtsNode::Term {
                                value: "baz".into(),
                                prefix: None,
                            },
                        ],
                    },
                ],
            })
        );
    }

    #[test]
    fn boolean_parser_preserves_phrases_prefixes_and_double_negation() {
        let parsed = parse_search_query(r#"NOT NOT "hello world" AND report*"#).unwrap();
        assert_eq!(
            parsed.fts_ast,
            Some(FtsNode::And {
                children: vec![
                    FtsNode::Not {
                        child: Box::new(FtsNode::Not {
                            child: Box::new(FtsNode::Phrase {
                                value: "hello world".into(),
                            }),
                        }),
                    },
                    FtsNode::Term {
                        value: "report".into(),
                        prefix: Some(true),
                    },
                ],
            })
        );
    }
}
