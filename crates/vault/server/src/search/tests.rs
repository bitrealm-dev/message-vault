//! Tests at the module's interface: seed a SQLite vault, compile a query
//! for a list, run it, and assert which ids come back.
//!
//! Every helper and every `Fixture` field is part of the shared fixture the
//! later tasks' test modules read, so the whole module opts out of the
//! dead-code lint rather than growing an attribute per unused name.
#![allow(dead_code)]

use chrono::NaiveDate;
use sqlx::AnyConnection;

use super::{CompileRequest, ListKind, QueryError, compile};
use crate::db::dialect::engine_of;
use crate::db::engine::DbEngine;
use crate::db::sql::{bind_args, renumber_placeholders};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-00000000aaaa";
pub(crate) const OTHER_ACCOUNT: &str = "00000000-0000-4000-8000-00000000bbbb";

pub(crate) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 2).unwrap()
}

/// Ids the fixture created, so a test can assert on them by name.
#[derive(Debug, Default)]
pub(crate) struct Fixture {
    // contacts
    pub ana: i64,
    pub bo: i64,
    pub cy: i64,
    pub jane: i64,
    pub sam: i64,
    pub nameless: i64,
    // handles
    pub me_handle: i64,
    pub ana_handle: i64,
    pub bo_handle: i64,
    pub jane_handle: i64,
    pub sam_handle: i64,
    pub nameless_handle: i64,
    // conversations
    pub ana_direct: i64,
    pub bo_direct: i64,
    pub jane_direct: i64,
    pub sam_direct: i64,
    pub archive_group: i64,
    pub big_group: i64,
    pub trashed_conv: i64,
    // messages
    pub ana_2018: i64,
    pub ana_2021: i64,
    pub bo_2023: i64,
    pub jane_avocado_from_me: i64,
    pub jane_guac_from_me: i64,
    pub jane_avocado_to_me: i64,
    pub sam_avocado_from_me: i64,
    pub jane_2018: i64,
    pub feb_big_jpeg: i64,
    pub feb_small_jpeg: i64,
    pub feb_pdf: i64,
    pub may_big_jpeg: i64,
    pub big_group_msg: i64,
    pub archive_msg: i64,
    pub trashed_msg: i64,
    // named sets
    pub family: i64,
    pub archive: i64,
}

pub(crate) async fn handle(
    conn: &mut AnyConnection,
    account: &str,
    raw: &str,
    service: &str,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO handles (account_id, raw, normalized, handle_type, service)
         VALUES ($1, $2, $2, 'phone', $3) RETURNING id",
    )
    .bind(account)
    .bind(raw)
    .bind(service)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

pub(crate) async fn contact(
    conn: &mut AnyConnection,
    account: &str,
    name: &str,
    handles: &[i64],
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, preferred_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(account)
    .bind(name)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for h in handles {
        sqlx::query(
            "INSERT INTO contact_handles (account_id, handle_id, contact_id) VALUES ($1, $2, $3)",
        )
        .bind(account)
        .bind(h)
        .bind(id)
        .execute(&mut *conn)
        .await
        .unwrap();
    }
    id
}

/// A conversation whose chat handle is `chat` and whose participants are the
/// given handles, each linked to its contact when one exists.
pub(crate) async fn conversation(
    conn: &mut AnyConnection,
    account: &str,
    chat: i64,
    kind: &str,
    title: Option<&str>,
    participants: &[i64],
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (account_id, chat_handle_id, conversation_type, group_title, source_file)
         VALUES ($1, $2, $3, $4, 'seed.jsonl') RETURNING id",
    )
    .bind(account)
    .bind(chat)
    .bind(kind)
    .bind(title)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for h in participants {
        let contact_id: Option<i64> = sqlx::query_scalar(
            "SELECT contact_id FROM contact_handles WHERE account_id = $1 AND handle_id = $2",
        )
        .bind(account)
        .bind(h)
        .fetch_optional(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO participants (conversation_id, handle_id, contact_id) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(h)
        .bind(contact_id)
        .execute(&mut *conn)
        .await
        .unwrap();
    }
    id
}

pub(crate) struct Msg<'a> {
    pub conversation: i64,
    pub timestamp: &'a str,
    pub from_me: bool,
    pub sender: Option<i64>,
    pub body: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub source: &'a str,
    pub service: &'a str,
}

pub(crate) fn msg<'a>(
    conversation: i64,
    timestamp: &'a str,
    from_me: bool,
    sender: Option<i64>,
    body: &'a str,
) -> Msg<'a> {
    Msg {
        conversation,
        timestamp,
        from_me,
        sender,
        body: Some(body),
        subject: None,
        source: "imessage",
        service: "imessage",
    }
}

pub(crate) async fn message(conn: &mut AnyConnection, account: &str, m: Msg<'_>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO messages (conversation_id, account_id, source, timestamp, is_from_me,
                               sender_handle_id, service, subject, body, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0) RETURNING id",
    )
    .bind(m.conversation)
    .bind(account)
    .bind(m.source)
    .bind(m.timestamp)
    .bind(i64::from(m.from_me))
    .bind(m.sender)
    .bind(m.service)
    .bind(m.subject)
    .bind(m.body)
    .fetch_one(&mut *conn)
    .await
    .unwrap()
}

pub(crate) async fn attachment(
    conn: &mut AnyConnection,
    message: i64,
    name: &str,
    mime: &str,
    size: i64,
) {
    sqlx::query(
        "INSERT INTO attachments (message_id, original_name, mime_type, size_bytes)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(message)
    .bind(name)
    .bind(mime)
    .bind(size)
    .execute(&mut *conn)
    .await
    .unwrap();
}

pub(crate) async fn group(
    conn: &mut AnyConnection,
    account: &str,
    name: &str,
    members: &[i64],
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO contact_groups (account_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(account)
    .bind(name)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for c in members {
        sqlx::query("INSERT INTO contact_group_members (contact_id, group_id) VALUES ($1, $2)")
            .bind(c)
            .bind(id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

pub(crate) async fn tag(
    conn: &mut AnyConnection,
    account: &str,
    name: &str,
    conversations: &[i64],
) -> i64 {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO message_tags (account_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(account)
    .bind(name)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    for c in conversations {
        sqlx::query("INSERT INTO message_tag_members (conversation_id, tag_id) VALUES ($1, $2)")
            .bind(c)
            .bind(id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    id
}

/// A vault with two accounts and every row the spec's cases need.
pub(crate) async fn seeded() -> (sqlx::AnyPool, tempfile::TempDir, Fixture) {
    let (pool, dir) = crate::db::engine::test_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    crate::db::schema::ensure_vault_schema(&mut conn)
        .await
        .unwrap();
    for (id, name) in [(ACCOUNT, "alice"), (OTHER_ACCOUNT, "bob")] {
        sqlx::query("INSERT INTO accounts (id, username) VALUES ($1, $2)")
            .bind(id)
            .bind(name)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let a = ACCOUNT;
    let mut f = Fixture {
        me_handle: handle(&mut conn, a, "+15550000", "imessage").await,
        ..Fixture::default()
    };

    sqlx::query("INSERT INTO account_handles (account_id, handle_id) VALUES ($1, $2)")
        .bind(a)
        .bind(f.me_handle)
        .execute(&mut *conn)
        .await
        .unwrap();
    f.ana_handle = handle(&mut conn, a, "+15550001", "imessage").await;
    f.bo_handle = handle(&mut conn, a, "+15550002", "sms").await;
    f.jane_handle = handle(&mut conn, a, "jane.doe@gmail.com", "imessage").await;
    f.sam_handle = handle(&mut conn, a, "sam@icloud.com", "imessage").await;
    f.nameless_handle = handle(&mut conn, a, "+15550009", "sms").await;
    let cy_handle = handle(&mut conn, a, "+15550003", "whatsapp").await;

    f.ana = contact(&mut conn, a, "Ana", &[f.ana_handle]).await;
    f.bo = contact(&mut conn, a, "Bo", &[f.bo_handle]).await;
    f.cy = contact(&mut conn, a, "Cy", &[cy_handle]).await;
    f.jane = contact(&mut conn, a, "Jane Doe", &[f.jane_handle]).await;
    f.sam = contact(&mut conn, a, "Sam", &[f.sam_handle]).await;
    f.nameless = contact(&mut conn, a, "", &[f.nameless_handle]).await;

    f.ana_direct = conversation(
        &mut conn,
        a,
        f.ana_handle,
        "individual",
        None,
        &[f.ana_handle],
    )
    .await;
    f.bo_direct = conversation(
        &mut conn,
        a,
        f.bo_handle,
        "individual",
        None,
        &[f.bo_handle],
    )
    .await;
    f.jane_direct = conversation(
        &mut conn,
        a,
        f.jane_handle,
        "individual",
        None,
        &[f.jane_handle],
    )
    .await;
    f.sam_direct = conversation(
        &mut conn,
        a,
        f.sam_handle,
        "individual",
        None,
        &[f.sam_handle],
    )
    .await;
    let archive_chat = handle(&mut conn, a, "chat100", "imessage").await;
    f.archive_group = conversation(
        &mut conn,
        a,
        archive_chat,
        "group",
        Some("Old Times"),
        &[f.ana_handle, f.bo_handle, f.sam_handle],
    )
    .await;
    let big_chat = handle(&mut conn, a, "chat200", "imessage").await;
    f.big_group = conversation(
        &mut conn,
        a,
        big_chat,
        "group",
        Some("Book Club"),
        &[f.ana_handle, f.bo_handle, f.jane_handle, f.sam_handle],
    )
    .await;
    let trashed_chat = handle(&mut conn, a, "chat300", "imessage").await;
    f.trashed_conv = conversation(
        &mut conn,
        a,
        trashed_chat,
        "group",
        Some("Gone"),
        &[f.ana_handle, f.bo_handle],
    )
    .await;
    sqlx::query("INSERT INTO trashed_conversations (account_id, conversation_id) VALUES ($1, $2)")
        .bind(a)
        .bind(f.trashed_conv)
        .execute(&mut *conn)
        .await
        .unwrap();

    f.ana_2018 = message(
        &mut conn,
        a,
        msg(
            f.ana_direct,
            "2018-03-01T10:00:00Z",
            false,
            Some(f.ana_handle),
            "hello from ana",
        ),
    )
    .await;
    f.ana_2021 = message(
        &mut conn,
        a,
        msg(f.ana_direct, "2021-05-01T10:00:00Z", true, None, "hi ana"),
    )
    .await;
    f.bo_2023 = message(
        &mut conn,
        a,
        Msg {
            service: "sms",
            ..msg(
                f.bo_direct,
                "2023-01-01T10:00:00Z",
                false,
                Some(f.bo_handle),
                "bo here",
            )
        },
    )
    .await;
    f.jane_avocado_from_me = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-10T10:00:00Z",
            true,
            None,
            "want some avocado",
        ),
    )
    .await;
    f.jane_guac_from_me = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-11T10:00:00Z",
            true,
            None,
            "guacamole night at mine",
        ),
    )
    .await;
    f.jane_avocado_to_me = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-12T10:00:00Z",
            false,
            Some(f.jane_handle),
            "avocado toast?",
        ),
    )
    .await;
    f.sam_avocado_from_me = message(
        &mut conn,
        a,
        msg(
            f.sam_direct,
            "2024-02-13T10:00:00Z",
            true,
            None,
            "avocado again",
        ),
    )
    .await;
    f.jane_2018 = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2018-06-01T10:00:00Z",
            false,
            Some(f.jane_handle),
            "first hello",
        ),
    )
    .await;
    f.feb_big_jpeg = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-20T10:00:00Z",
            false,
            Some(f.jane_handle),
            "photo",
        ),
    )
    .await;
    attachment(
        &mut conn,
        f.feb_big_jpeg,
        "beach.jpg",
        "image/jpeg",
        900 * 1024,
    )
    .await;
    f.feb_small_jpeg = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-21T10:00:00Z",
            false,
            Some(f.jane_handle),
            "small photo",
        ),
    )
    .await;
    attachment(
        &mut conn,
        f.feb_small_jpeg,
        "thumb.jpg",
        "image/jpeg",
        100 * 1024,
    )
    .await;
    f.feb_pdf = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-02-22T10:00:00Z",
            false,
            Some(f.jane_handle),
            "the document",
        ),
    )
    .await;
    attachment(
        &mut conn,
        f.feb_pdf,
        "notes.pdf",
        "application/pdf",
        2 * 1024 * 1024,
    )
    .await;
    f.may_big_jpeg = message(
        &mut conn,
        a,
        msg(
            f.jane_direct,
            "2024-05-20T10:00:00Z",
            false,
            Some(f.jane_handle),
            "later photo",
        ),
    )
    .await;
    attachment(
        &mut conn,
        f.may_big_jpeg,
        "hike.jpg",
        "image/jpeg",
        900 * 1024,
    )
    .await;
    f.big_group_msg = message(
        &mut conn,
        a,
        Msg {
            subject: Some("Dinner plans"),
            ..msg(
                f.big_group,
                "2024-03-01T10:00:00Z",
                false,
                Some(f.sam_handle),
                "who is in",
            )
        },
    )
    .await;
    f.archive_msg = message(
        &mut conn,
        a,
        Msg {
            source: "whatsapp",
            service: "whatsapp",
            ..msg(
                f.archive_group,
                "2019-01-01T10:00:00Z",
                false,
                Some(f.bo_handle),
                "old",
            )
        },
    )
    .await;
    f.trashed_msg = message(
        &mut conn,
        a,
        msg(
            f.trashed_conv,
            "2019-02-01T10:00:00Z",
            false,
            Some(f.bo_handle),
            "gone",
        ),
    )
    .await;

    f.family = group(&mut conn, a, "Family", &[f.ana]).await;
    f.archive = tag(&mut conn, a, "Archive", &[f.archive_group]).await;

    // The other account has one contact and one message that must never show.
    let other_handle = handle(&mut conn, OTHER_ACCOUNT, "+15559999", "imessage").await;
    contact(&mut conn, OTHER_ACCOUNT, "Ana", &[other_handle]).await;
    let other_conv = conversation(
        &mut conn,
        OTHER_ACCOUNT,
        other_handle,
        "individual",
        None,
        &[other_handle],
    )
    .await;
    message(
        &mut conn,
        OTHER_ACCOUNT,
        msg(
            other_conv,
            "2024-02-10T10:00:00Z",
            false,
            Some(other_handle),
            "avocado",
        ),
    )
    .await;

    drop(conn);
    (pool, dir, f)
}

/// Compile `q` for `list` and return the matching ids, ascending.
pub(crate) async fn run(conn: &mut AnyConnection, list: ListKind, q: &str) -> Vec<i64> {
    let f = compile(CompileRequest {
        list,
        query: q,
        account_id: ACCOUNT,
        engine: engine_of(conn),
        today: today(),
    })
    .unwrap_or_else(|e| panic!("{q:?} on {list:?}: {}", e.message));
    let table = match list {
        ListKind::Contacts => "contacts",
        ListKind::Conversations => "conversations",
        ListKind::Messages => "messages",
    };
    let alias = list.base_alias();
    let sql = renumber_placeholders(&format!(
        "SELECT {alias}.id FROM {table} {alias} WHERE {} ORDER BY {alias}.id",
        f.where_sql()
    ));
    let rows: Vec<i64> = sqlx::query_scalar_with(&sql, bind_args(f.params()))
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("{q:?} on {list:?}: {e}\n{sql}"));
    rows
}

pub(crate) fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

/// Compile `q` for `list` expecting a refusal.
pub(crate) fn err(list: ListKind, q: &str) -> QueryError {
    compile(CompileRequest {
        list,
        query: q,
        account_id: ACCOUNT,
        engine: DbEngine::Sqlite,
        today: today(),
    })
    .expect_err("expected a refusal")
}

mod free_text {
    use super::*;

    #[tokio::test]
    async fn empty_query_returns_the_account_rows_minus_trash() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "").await,
            sorted(vec![f.ana, f.bo, f.cy, f.jane, f.sam, f.nameless])
        );
        let convs = run(&mut conn, ListKind::Conversations, "").await;
        assert!(!convs.contains(&f.trashed_conv));
        assert_eq!(convs.len(), 6);
        let msgs = run(&mut conn, ListKind::Messages, "").await;
        assert!(msgs.contains(&f.ana_2018));
        assert!(!msgs.contains(&f.trashed_msg));
        assert!(
            msgs.iter().all(|id| *id <= f.trashed_msg),
            "other account's message leaked"
        );
    }

    #[tokio::test]
    async fn bare_text_is_the_rows_own_text() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Contacts: name or handle.
        assert_eq!(run(&mut conn, ListKind::Contacts, "ana").await, vec![f.ana]);
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "gmail").await,
            vec![f.jane]
        );
        // Conversations: title, or a participant's name or handle.
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "book").await,
            vec![f.big_group]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "jane").await,
            sorted(vec![f.jane_direct, f.big_group])
        );
        // Messages: body, subject, attachment names, through the full-text index.
        assert_eq!(
            run(&mut conn, ListKind::Messages, "avocado").await,
            sorted(vec![
                f.jane_avocado_from_me,
                f.jane_avocado_to_me,
                f.sam_avocado_from_me
            ])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "dinner").await,
            vec![f.big_group_msg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "beach").await,
            vec![f.feb_big_jpeg]
        );
        // A person's name is not message text.
        assert_eq!(
            run(&mut conn, ListKind::Messages, "jane").await,
            Vec::<i64>::new()
        );
    }

    #[tokio::test]
    async fn phrases_prefixes_negation_and_or() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Messages, "\"guacamole night\"").await,
            vec![f.jane_guac_from_me]
        );
        assert_eq!(run(&mut conn, ListKind::Messages, "avoc*").await.len(), 3);
        assert_eq!(
            run(&mut conn, ListKind::Messages, "avocado -toast").await,
            sorted(vec![f.jane_avocado_from_me, f.sam_avocado_from_me])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "toast or guacamole").await,
            sorted(vec![f.jane_guac_from_me, f.jane_avocado_to_me])
        );
        assert_eq!(
            run(
                &mut conn,
                ListKind::Messages,
                "(toast or guacamole) avocado"
            )
            .await,
            vec![f.jane_avocado_to_me]
        );
    }

    #[test]
    fn compile_is_deterministic_and_pure() {
        let req = || CompileRequest {
            list: ListKind::Messages,
            query: "avocado -toast",
            account_id: ACCOUNT,
            engine: DbEngine::Postgres,
            today: today(),
        };
        let a = compile(req()).unwrap();
        let b = compile(req()).unwrap();
        assert_eq!(a.where_sql(), b.where_sql());
        assert_eq!(a.params(), b.params());
    }

    #[test]
    fn a_fragment_mentions_only_its_base_alias_and_binds_in_order() {
        let f = compile(CompileRequest {
            list: ListKind::Contacts,
            query: "ana",
            account_id: ACCOUNT,
            engine: DbEngine::Sqlite,
            today: today(),
        })
        .unwrap();
        assert!(f.where_sql().starts_with("(ct.account_id = ?"));
        assert_eq!(f.where_sql().matches('?').count(), f.params().len());
    }
}
