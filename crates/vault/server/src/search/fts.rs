//! The full-text leaf for a free-text term on Messages. SQLite uses the
//! contentless FTS5 table; Postgres uses the `search_tsv` column. Both index
//! body, subject, attachment names, and transcriptions.

use crate::db::engine::DbEngine;

use super::bridge::Sql;
use super::parse::TextTerm;

/// Quote for FTS5 so operators and punctuation are literal text.
fn fts5_literal(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// `'term':*` for `to_tsquery`, or `None` when the term holds a quote or
/// backslash, which tsquery literals cannot carry.
fn pg_prefix(term: &str) -> Option<String> {
    if term.is_empty() || term.contains(['\\', '\'']) {
        return None;
    }
    Some(format!("'{term}':*"))
}

/// Message `m` matches `term` in the full-text index.
pub(crate) fn leaf(out: &mut Sql, engine: DbEngine, term: &TextTerm) {
    match engine {
        DbEngine::Sqlite => {
            let q = match term {
                TextTerm::Term { text, prefix: true } => format!("{}*", fts5_literal(text)),
                TextTerm::Term {
                    text,
                    prefix: false,
                } => fts5_literal(text),
                TextTerm::Phrase(text) => fts5_literal(text),
            };
            out.push(
                "EXISTS (SELECT 1 FROM messages_fts fts WHERE fts.rowid = m.id AND messages_fts MATCH ",
            );
            out.bind_text(q);
            out.push(")");
        }
        DbEngine::Postgres => {
            let (func, arg) = match term {
                TextTerm::Term { text, prefix: true } => match pg_prefix(text) {
                    Some(q) => ("to_tsquery", q),
                    None => ("plainto_tsquery", text.clone()),
                },
                TextTerm::Term {
                    text,
                    prefix: false,
                } => ("plainto_tsquery", text.clone()),
                TextTerm::Phrase(text) => ("phraseto_tsquery", text.clone()),
            };
            out.push(&format!("m.search_tsv @@ {func}('simple', "));
            out.bind_text(arg);
            out.push(")");
        }
    }
}
