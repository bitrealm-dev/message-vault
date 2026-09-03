//! Query string to tokens. Every token carries the byte range it came from,
//! so an error can point at the exact text.

use std::ops::Range;

use super::error::{QueryError, QueryErrorKind};

/// Reject huge query strings before doing anything else.
pub(crate) const MAX_QUERY_BYTES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    LParen,
    RParen,
    Or,
    And,
    Not,
    /// `word:value`. `quoted` says the value came in quotes, so a comma in it
    /// is text rather than a list separator.
    Field {
        word: String,
        value: String,
        quoted: bool,
    },
    /// A bare word; `prefix` when it ended in `*`.
    Word {
        text: String,
        prefix: bool,
    },
    /// A quoted phrase.
    Phrase(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
    /// A leading `-` was attached to this token.
    pub negated: bool,
}

/// Read a quoted value starting just after the opening quote. `""` inside is
/// one literal quote. Returns the text and the index just past the closing
/// quote, or `None` when the quote never closes.
fn read_quoted(bytes: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                out.push(b'"');
                i += 2;
                continue;
            }
            return Some((String::from_utf8_lossy(&out).into_owned(), i + 1));
        }
        out.push(bytes[i]);
        i += 1;
    }
    None
}

fn is_field_word(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphabetic() || c == '-')
}

fn is_bare_end(b: u8) -> bool {
    b.is_ascii_whitespace() || b == b'(' || b == b')'
}

/// Tokenize `input`.
///
/// # Errors
///
/// `TooLong` past [`MAX_QUERY_BYTES`]; `Unbalanced` for a quote that never closes.
pub(crate) fn tokenize(input: &str) -> Result<Vec<Token>, QueryError> {
    if input.len() > MAX_QUERY_BYTES {
        return Err(QueryError::new(
            QueryErrorKind::TooLong,
            0..input.len(),
            format!("The search is longer than {MAX_QUERY_BYTES} bytes."),
        ));
    }
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        if bytes[i] == b'(' || bytes[i] == b')' {
            let kind = if bytes[i] == b'(' {
                TokenKind::LParen
            } else {
                TokenKind::RParen
            };
            tokens.push(Token {
                kind,
                span: start..i + 1,
                negated: false,
            });
            i += 1;
            continue;
        }
        let mut negated = false;
        if bytes[i] == b'-' && i + 1 < bytes.len() && !is_bare_end(bytes[i + 1]) {
            negated = true;
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'(' {
            // `-(a or b)`: the minus applies to the group.
            tokens.push(Token {
                kind: TokenKind::LParen,
                span: start..i + 1,
                negated,
            });
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            let (text, next) = read_quoted(bytes, i + 1).ok_or_else(|| {
                QueryError::new(
                    QueryErrorKind::Unbalanced,
                    i..bytes.len(),
                    "A quote never closes.",
                )
            })?;
            tokens.push(Token {
                kind: TokenKind::Phrase(text),
                span: start..next,
                negated,
            });
            i = next;
            continue;
        }
        // A bare run up to whitespace or a parenthesis, watching for `word:`.
        let mut j = i;
        while j < bytes.len() && !is_bare_end(bytes[j]) && bytes[j] != b':' {
            j += 1;
        }
        let head = &input[i..j];
        // `word:` is a field unless the value starts with `/`, so a pasted
        // URL such as `http://x` stays a word.
        let value_starts_with_slash = j + 1 < bytes.len() && bytes[j + 1] == b'/';
        if j < bytes.len() && bytes[j] == b':' && is_field_word(head) && !value_starts_with_slash {
            let word = head.to_ascii_lowercase();
            let mut k = j + 1;
            if k < bytes.len() && bytes[k] == b'"' {
                let (value, next) = read_quoted(bytes, k + 1).ok_or_else(|| {
                    QueryError::new(
                        QueryErrorKind::Unbalanced,
                        k..bytes.len(),
                        "A quote never closes.",
                    )
                })?;
                tokens.push(Token {
                    kind: TokenKind::Field {
                        word,
                        value,
                        quoted: true,
                    },
                    span: start..next,
                    negated,
                });
                i = next;
                continue;
            }
            while k < bytes.len() && !is_bare_end(bytes[k]) {
                k += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Field {
                    word,
                    value: input[j + 1..k].to_string(),
                    quoted: false,
                },
                span: start..k,
                negated,
            });
            i = k;
            continue;
        }
        // Not a field: take the whole bare run, colons included.
        while j < bytes.len() && !is_bare_end(bytes[j]) {
            j += 1;
        }
        let text = &input[i..j];
        let lower = text.to_ascii_lowercase();
        let kind = match lower.as_str() {
            "or" if !negated => TokenKind::Or,
            "and" if !negated => TokenKind::And,
            "not" if !negated => TokenKind::Not,
            _ => {
                let (text, prefix) = match text.strip_suffix('*') {
                    Some(t) if !t.is_empty() => (t.to_string(), true),
                    _ => (text.to_string(), false),
                };
                TokenKind::Word { text, prefix }
            }
        };
        tokens.push(Token {
            kind,
            span: start..j,
            negated,
        });
        i = j;
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn words_phrases_and_prefixes() {
        assert_eq!(
            kinds(r#"hello "two words" avoc*"#),
            vec![
                TokenKind::Word {
                    text: "hello".into(),
                    prefix: false
                },
                TokenKind::Phrase("two words".into()),
                TokenKind::Word {
                    text: "avoc".into(),
                    prefix: true
                },
            ]
        );
    }

    #[test]
    fn fields_take_bare_and_quoted_values() {
        assert_eq!(
            kinds(r#"tag:Work group:"Book Club" date:2019..2021"#),
            vec![
                TokenKind::Field {
                    word: "tag".into(),
                    value: "Work".into(),
                    quoted: false
                },
                TokenKind::Field {
                    word: "group".into(),
                    value: "Book Club".into(),
                    quoted: true
                },
                TokenKind::Field {
                    word: "date".into(),
                    value: "2019..2021".into(),
                    quoted: false
                },
            ]
        );
    }

    #[test]
    fn a_doubled_quote_is_a_literal_quote() {
        assert_eq!(
            kinds(r#"title:"say ""hi"" now""#),
            vec![TokenKind::Field {
                word: "title".into(),
                value: r#"say "hi" now"#.into(),
                quoted: true
            }]
        );
    }

    #[test]
    fn operators_and_negation() {
        let toks = tokenize("-tag:Work or (a and not b)").unwrap();
        assert!(toks[0].negated);
        assert!(matches!(toks[0].kind, TokenKind::Field { .. }));
        assert_eq!(toks[1].kind, TokenKind::Or);
        assert_eq!(toks[2].kind, TokenKind::LParen);
        assert_eq!(toks[4].kind, TokenKind::And);
        assert_eq!(toks[5].kind, TokenKind::Not);
        assert_eq!(toks[7].kind, TokenKind::RParen);
        // Operator words are case-insensitive.
        assert_eq!(kinds("OR")[0], TokenKind::Or);
    }

    #[test]
    fn spans_are_byte_ranges_into_the_input() {
        let toks = tokenize("hello tag:Work").unwrap();
        assert_eq!(toks[0].span, 0..5);
        assert_eq!(toks[1].span, 6..14);
        // A negated token's span starts at the minus.
        let toks = tokenize("a -b").unwrap();
        assert_eq!(toks[1].span, 2..4);
    }

    #[test]
    fn a_field_word_is_lowercase_letters_and_hyphens() {
        // `Re:` inside a phrase is text, and a token like `http://x` is a word,
        // not a field, because the part before the colon is not a field shape.
        assert_eq!(
            kinds("http://x")[0],
            TokenKind::Word {
                text: "http://x".into(),
                prefix: false
            }
        );
        assert_eq!(
            kinds("First-Message:2019")[0],
            TokenKind::Field {
                word: "first-message".into(),
                value: "2019".into(),
                quoted: false
            }
        );
    }

    #[test]
    fn an_empty_field_value_is_kept_for_the_parser_to_reject() {
        assert_eq!(
            kinds("tag:")[0],
            TokenKind::Field {
                word: "tag".into(),
                value: String::new(),
                quoted: false
            }
        );
    }

    #[test]
    fn unterminated_quote_is_unbalanced() {
        let err = tokenize(r#"tag:"Book"#).unwrap_err();
        assert_eq!(err.kind, QueryErrorKind::Unbalanced);
        assert_eq!(err.span, 4..9);
    }

    #[test]
    fn too_long_is_refused_before_anything_else() {
        let long = "a".repeat(MAX_QUERY_BYTES + 1);
        assert_eq!(tokenize(&long).unwrap_err().kind, QueryErrorKind::TooLong);
    }
}
