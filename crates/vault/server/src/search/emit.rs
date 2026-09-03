//! Defaults plus one emitter per word. Every emitter writes SQL against the
//! innermost alias it needs and lets `ListCtx` wrap it for the base row.

use crate::db::engine::DbEngine;

use super::bridge::{ListCtx, Sql};
use super::error::{QueryError, QueryErrorKind};
use super::fts;
use super::parse::{Expr, FieldTerm, TextTerm};
use super::{Filter, ListKind};

/// Contact `ct` is not in the trash.
pub(crate) const NOT_TRASHED_CONTACT: &str = "NOT EXISTS (SELECT 1 FROM trashed_contacts tct WHERE tct.account_id = ct.account_id AND tct.contact_id = ct.id)";
/// Conversation `c` is not in the trash and neither is its chat handle.
pub(crate) const NOT_TRASHED_CONVERSATION: &str = "NOT EXISTS (SELECT 1 FROM trashed_conversations tc WHERE tc.account_id = c.account_id AND tc.conversation_id = c.id) \
     AND NOT EXISTS (SELECT 1 FROM trashed_handles th WHERE th.account_id = c.account_id AND th.handle_id = c.chat_handle_id)";

/// Compile a parsed query into one parenthesised WHERE fragment.
pub(crate) fn compile(
    list: ListKind,
    expr: Option<&Expr>,
    account_id: &str,
    engine: DbEngine,
) -> Result<Filter, QueryError> {
    let ctx = ListCtx {
        list,
        engine,
        account_id,
    };
    let mut out = Sql::default();
    out.push("(");
    out.push(ctx.account_col());
    out.push(" = ");
    out.bind_text(ctx.account_id);
    let uses = |word: &str| expr.is_some_and(|e| e.uses(word));
    match list {
        ListKind::Contacts => {
            if !uses("trashed") {
                out.push(" AND ");
                out.push(NOT_TRASHED_CONTACT);
            }
        }
        ListKind::Conversations => {
            if !uses("trashed") {
                out.push(" AND ");
                out.push(NOT_TRASHED_CONVERSATION);
            }
            // A thread with only duplicate messages is hidden, unless the
            // query is about an Import Run, whose threads may be exactly that.
            if !uses("import") {
                out.push(
                    " AND EXISTS (SELECT 1 FROM messages m0 WHERE m0.conversation_id = c.id AND m0.duplicate_of IS NULL)",
                );
            }
        }
        ListKind::Messages => {
            // A query about one source wants that source's copies, duplicates included.
            if !uses("source") {
                out.push(" AND m.duplicate_of IS NULL");
            }
            out.push(
                " AND EXISTS (SELECT 1 FROM conversations c WHERE c.id = m.conversation_id AND ",
            );
            out.push(NOT_TRASHED_CONVERSATION);
            out.push(")");
        }
    }
    if let Some(expr) = expr {
        out.push(" AND ");
        emit_expr(&ctx, &mut out, expr)?;
    }
    out.push(")");
    Ok(Filter {
        where_sql: out.text,
        params: out.params,
    })
}

fn emit_expr(ctx: &ListCtx<'_>, out: &mut Sql, expr: &Expr) -> Result<(), QueryError> {
    match expr {
        Expr::And(parts) | Expr::Or(parts) => {
            let joiner = if matches!(expr, Expr::And(_)) {
                " AND "
            } else {
                " OR "
            };
            out.push("(");
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(joiner);
                }
                emit_expr(ctx, out, part)?;
            }
            out.push(")");
        }
        Expr::Not(inner) => {
            out.push("NOT (");
            emit_expr(ctx, out, inner)?;
            out.push(")");
        }
        Expr::Text(term) => emit_text(ctx, out, term),
        Expr::Field(term) => emit_field(ctx, out, term)?,
    }
    Ok(())
}

/// One text test on `column`, for the lists matched with LIKE.
///
/// A prefix term means "a word starts with this", so it matches at the start
/// of the column or just after a space — never only at the very start, which
/// would make `avoc*` find less than `avoc`. Anything else, a phrase
/// included, is an ordinary substring.
fn text_match(out: &mut Sql, engine: DbEngine, column: &str, term: &TextTerm) {
    match term {
        TextTerm::Term { text, prefix: true } => {
            out.push("(");
            out.like(engine, column, &format!("{text}%"));
            out.push(" OR ");
            out.like(engine, column, &format!("% {text}%"));
            out.push(")");
        }
        TextTerm::Term {
            text,
            prefix: false,
        }
        | TextTerm::Phrase(text) => out.like(engine, column, &format!("%{text}%")),
    }
}

/// Free text: the row's own text, one meaning applied per row type.
fn emit_text(ctx: &ListCtx<'_>, out: &mut Sql, term: &TextTerm) {
    let e = ctx.engine;
    match ctx.list {
        ListKind::Contacts => {
            out.push("(");
            text_match(
                out,
                e,
                "COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)')",
                term,
            );
            out.push(
                " OR EXISTS (SELECT 1 FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id AND (",
            );
            text_match(out, e, "h.raw", term);
            out.push(" OR ");
            text_match(out, e, "coalesce(h.normalized, '')", term);
            out.push(")))");
        }
        ListKind::Conversations => {
            out.push("(");
            text_match(out, e, "coalesce(c.group_title, '')", term);
            out.push(" OR EXISTS (SELECT 1 FROM handles hc WHERE hc.id = c.chat_handle_id AND ");
            text_match(out, e, "hc.raw", term);
            // The handle join is a LEFT join: a source may name a participant
            // and record no address for them, and that person is searchable by
            // the name the source gave.
            out.push(
                ") OR EXISTS (SELECT 1 FROM participants p LEFT JOIN handles ph ON ph.id = p.handle_id LEFT JOIN contacts pct ON pct.id = p.contact_id WHERE p.conversation_id = c.id AND (",
            );
            text_match(out, e, "coalesce(ph.raw, '')", term);
            out.push(" OR ");
            text_match(out, e, "coalesce(p.name_alias, '')", term);
            out.push(" OR ");
            text_match(out, e, "coalesce(pct.preferred_name, '')", term);
            out.push(")))");
        }
        ListKind::Messages => fts::leaf(out, e, term),
    }
}

/// One `word:values`. Values are OR-ed. Tasks 5 to 8 add the arms; until a
/// word has one, the query is refused by name rather than quietly matching.
fn emit_field(ctx: &ListCtx<'_>, out: &mut Sql, term: &FieldTerm) -> Result<(), QueryError> {
    let _ = (ctx, out);
    Err(QueryError::new(
        QueryErrorKind::BadValue,
        term.span.clone(),
        format!("{}: is not built yet.", term.spec.word),
    ))
}
