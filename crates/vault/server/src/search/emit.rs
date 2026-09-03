//! Defaults plus one emitter per word. Every emitter writes SQL against the
//! innermost alias it needs and lets `ListCtx` wrap it for the base row.

use crate::db::engine::DbEngine;

use super::bridge::{ListCtx, Sql};
use super::error::{QueryError, QueryErrorKind};
use super::fts;
use super::parse::{Expr, FieldTerm, TextTerm};
use super::value::Value;
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

/// A contains-or-prefix pattern against `column`, case-insensitive on both
/// engines. A prefix means "a word starts with this", so it matches at the
/// start of the column or just after a space — never only at the very
/// start, which would make `avoc*` find less than `avoc`. Anything else, a
/// phrase included, is an ordinary substring.
///
/// The one place that turns text or a prefix into a LIKE pattern: free text
/// (`free_text_match`) and the text words (`text_match`) both go through it.
fn like_contains(out: &mut Sql, engine: DbEngine, column: &str, text: &str, prefix: bool) {
    if prefix {
        out.push("(");
        out.like(engine, column, &format!("{text}%"));
        out.push(" OR ");
        out.like(engine, column, &format!("% {text}%"));
        out.push(")");
    } else {
        out.like(engine, column, &format!("%{text}%"));
    }
}

/// One free-text test on `column`, for the lists matched with LIKE.
fn free_text_match(out: &mut Sql, engine: DbEngine, column: &str, term: &TextTerm) {
    match term {
        TextTerm::Term { text, prefix } => like_contains(out, engine, column, text, *prefix),
        TextTerm::Phrase(text) => like_contains(out, engine, column, text, false),
    }
}

/// A participant's display name: the linked contact's name, else the
/// per-conversation alias. Alias `p` is a participants row, `pct` its
/// contact. Shared by free text and `name:` so there is one copy of what
/// "this participant's name" means.
const PARTICIPANT_NAME: &str =
    "coalesce(NULLIF(trim(pct.preferred_name), ''), NULLIF(trim(p.name_alias), ''), '')";

/// Free text: the row's own text, one meaning applied per row type.
fn emit_text(ctx: &ListCtx<'_>, out: &mut Sql, term: &TextTerm) {
    let e = ctx.engine;
    match ctx.list {
        ListKind::Contacts => {
            out.push("(");
            free_text_match(
                out,
                e,
                "COALESCE(NULLIF(trim(ct.preferred_name), ''), '(unknown)')",
                term,
            );
            out.push(
                " OR EXISTS (SELECT 1 FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id AND (",
            );
            free_text_match(out, e, "h.raw", term);
            out.push(" OR ");
            free_text_match(out, e, "coalesce(h.normalized, '')", term);
            out.push(")))");
        }
        ListKind::Conversations => {
            out.push("(");
            free_text_match(out, e, "coalesce(c.group_title, '')", term);
            out.push(" OR EXISTS (SELECT 1 FROM handles hc WHERE hc.id = c.chat_handle_id AND ");
            free_text_match(out, e, "hc.raw", term);
            // The handle join is a LEFT join: a source may name a participant
            // and record no address for them, and that person is searchable by
            // the name the source gave.
            out.push(
                ") OR EXISTS (SELECT 1 FROM participants p LEFT JOIN handles ph ON ph.id = p.handle_id LEFT JOIN contacts pct ON pct.id = p.contact_id WHERE p.conversation_id = c.id AND (",
            );
            free_text_match(out, e, "coalesce(ph.raw, '')", term);
            out.push(" OR ");
            free_text_match(out, e, PARTICIPANT_NAME, term);
            out.push(")))");
        }
        ListKind::Messages => fts::leaf(out, e, term),
    }
}

/// One `word:values`. The values are OR-ed, so `body:avocado,guac` is
/// either. Tasks 6 to 8 add the remaining words; until a word has an arm in
/// `emit_one`, the query is refused by name rather than quietly matching.
fn emit_field(ctx: &ListCtx<'_>, out: &mut Sql, term: &FieldTerm) -> Result<(), QueryError> {
    out.push("(");
    for (i, value) in term.values.iter().enumerate() {
        if i > 0 {
            out.push(" OR ");
        }
        emit_one(ctx, out, term, value)?;
    }
    out.push(")");
    Ok(())
}

/// One value of one word, written against the innermost alias it needs.
fn emit_one(
    ctx: &ListCtx<'_>,
    out: &mut Sql,
    term: &FieldTerm,
    v: &Value,
) -> Result<(), QueryError> {
    match term.spec.word {
        "body" | "subject" | "name" | "title" | "handle" | "filename" => {
            emit_text_word(ctx, out, term, v)
        }
        _ => Err(QueryError::new(
            QueryErrorKind::BadValue,
            term.span.clone(),
            format!("{}: is not built yet.", term.spec.word),
        )),
    }
}

/// `column` contains `v` (a text value), starts with it (a prefix value),
/// is empty (`none`), or is not empty (`any`). The LIKE pattern itself is
/// `like_contains`, shared with free text.
///
/// The other `Value` shapes never reach a text word: the parser only ever
/// hands a `Text`-typed word a `Text`, a `Prefix`, or one of its own
/// `values` keywords (`none`/`any`, or nothing for a word like `filename`
/// that declares neither). That arm exists only so the match is exhaustive;
/// it refuses by name rather than emitting a fallback that quietly matches
/// everything or nothing.
fn text_match(
    out: &mut Sql,
    engine: DbEngine,
    column: &str,
    term: &FieldTerm,
    v: &Value,
) -> Result<(), QueryError> {
    match v {
        Value::Text(t) => {
            like_contains(out, engine, column, t, false);
            Ok(())
        }
        Value::Prefix(p) => {
            like_contains(out, engine, column, p, true);
            Ok(())
        }
        Value::Keyword("none") => {
            out.push(&format!("NULLIF(trim({column}), '') IS NULL"));
            Ok(())
        }
        Value::Keyword("any") => {
            out.push(&format!("NULLIF(trim({column}), '') IS NOT NULL"));
            Ok(())
        }
        _ => Err(QueryError::new(
            QueryErrorKind::BadValue,
            term.span.clone(),
            format!("{}: needs text, a prefix, or none/any.", term.spec.word),
        )),
    }
}

/// The six text words. On Contacts, `name:` and `handle:` look at the
/// contact itself; everywhere else they look at the conversation's
/// participants. `body:`, `subject:`, and `filename:` always look at
/// messages (and their attachments); `title:` always looks at the
/// conversation.
fn emit_text_word(
    ctx: &ListCtx<'_>,
    out: &mut Sql,
    term: &FieldTerm,
    v: &Value,
) -> Result<(), QueryError> {
    let e = ctx.engine;
    let mut result: Result<(), QueryError> = Ok(());
    match (term.spec.word, ctx.list) {
        ("body", _) => ctx.message(out, |o| {
            result = text_match(o, e, "coalesce(m.body, '')", term, v);
        }),
        ("subject", _) => ctx.message(out, |o| {
            result = text_match(o, e, "coalesce(m.subject, '')", term, v);
        }),
        ("title", _) => ctx.conversation(out, |o| {
            result = text_match(o, e, "coalesce(c.group_title, '')", term, v);
        }),
        ("name", ListKind::Contacts) => {
            result = text_match(out, e, "ct.preferred_name", term, v);
        }
        ("name", _) => ctx.conversation(out, |o| {
            o.push(
                "EXISTS (SELECT 1 FROM participants p LEFT JOIN contacts pct ON pct.id = p.contact_id WHERE p.conversation_id = c.id AND ",
            );
            result = text_match(o, e, PARTICIPANT_NAME, term, v);
            o.push(")");
        }),
        ("handle", ListKind::Contacts) => match v {
            Value::Keyword("none") => out.push(
                "NOT EXISTS (SELECT 1 FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id)",
            ),
            Value::Keyword("any") => out.push(
                "EXISTS (SELECT 1 FROM contact_handles ch WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id)",
            ),
            Value::Text(_) | Value::Prefix(_) => {
                out.push(
                    "EXISTS (SELECT 1 FROM contact_handles ch JOIN handles h ON h.id = ch.handle_id WHERE ch.account_id = ct.account_id AND ch.contact_id = ct.id AND (",
                );
                result = text_match(out, e, "h.raw", term, v);
                out.push(" OR ");
                if result.is_ok() {
                    result = text_match(out, e, "coalesce(h.normalized, '')", term, v);
                }
                out.push("))");
            }
            _ => {
                result = Err(QueryError::new(
                    QueryErrorKind::BadValue,
                    term.span.clone(),
                    format!("{}: needs text, a prefix, or none/any.", term.spec.word),
                ));
            }
        },
        ("handle", _) => ctx.conversation(out, |o| match v {
            Value::Keyword("none") => o.push(
                "NOT EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND p.handle_id IS NOT NULL)",
            ),
            // The true complement of `none`: some participant does have a
            // handle. Never `1=1` — a conversation can be all name-only
            // participants (see `named_participant` in the fixture), and
            // `any` must not match those.
            Value::Keyword("any") => o.push(
                "EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND p.handle_id IS NOT NULL)",
            ),
            Value::Text(_) | Value::Prefix(_) => {
                o.push(
                    "EXISTS (SELECT 1 FROM handles h WHERE (h.id = c.chat_handle_id OR EXISTS (SELECT 1 FROM participants p WHERE p.conversation_id = c.id AND p.handle_id = h.id)) AND (",
                );
                result = text_match(o, e, "h.raw", term, v);
                o.push(" OR ");
                if result.is_ok() {
                    result = text_match(o, e, "coalesce(h.normalized, '')", term, v);
                }
                o.push("))");
            }
            _ => {
                result = Err(QueryError::new(
                    QueryErrorKind::BadValue,
                    term.span.clone(),
                    format!("{}: needs text, a prefix, or none/any.", term.spec.word),
                ));
            }
        }),
        ("filename", _) => ctx.message(out, |o| {
            o.push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND ");
            result = text_match(o, e, "coalesce(a.original_name, '')", term, v);
            o.push(")");
        }),
        _ => {
            result = Err(QueryError::new(
                QueryErrorKind::BadValue,
                term.span.clone(),
                format!("{}: is not built yet.", term.spec.word),
            ));
        }
    }
    result
}
