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
    // a conversation whose only message a later import marked as a
    // duplicate: invisible to an ordinary query, findable only by `import:`.
    pub dup_only_conv: i64,
    pub dup_only_msg: i64,
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

/// A participant the source named but gave no address for: `handle_id` is
/// NULL and `name_alias` carries who they are.
pub(crate) async fn named_participant(conn: &mut AnyConnection, conversation: i64, alias: &str) {
    sqlx::query(
        "INSERT INTO participants (conversation_id, handle_id, contact_id, name_alias)
         VALUES ($1, NULL, NULL, $2)",
    )
    .bind(conversation)
    .bind(alias)
    .execute(&mut *conn)
    .await
    .unwrap();
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
    // The source named this one and gave no address for them.
    named_participant(&mut conn, f.archive_group, "Robin").await;
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

    // A conversation whose only message a later import superseded: marked a
    // duplicate, with no other kept copy in its conversation. The handle
    // links to no contact, so this adds no contact-level count.
    let dup_only_handle = handle(&mut conn, a, "+15550098", "sms").await;
    f.dup_only_conv = conversation(
        &mut conn,
        a,
        dup_only_handle,
        "individual",
        None,
        &[dup_only_handle],
    )
    .await;
    f.dup_only_msg = message(
        &mut conn,
        a,
        msg(
            f.dup_only_conv,
            "2025-01-01T10:00:00Z",
            false,
            Some(dup_only_handle),
            "resent copy",
        ),
    )
    .await;
    sqlx::query("UPDATE messages SET duplicate_of = $1 WHERE id = $2")
        .bind(f.dup_only_msg)
        .bind(f.dup_only_msg)
        .execute(&mut *conn)
        .await
        .unwrap();

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

    #[tokio::test]
    async fn a_participant_the_source_only_named_is_still_searchable() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // "Robin" is in Old Times with no address of their own.
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "robin").await,
            vec![f.archive_group]
        );
    }

    #[tokio::test]
    async fn a_prefix_matches_any_word_that_starts_with_it() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // The prefix starts Jane Doe's second word, not her first.
        let by_prefix = run(&mut conn, ListKind::Contacts, "doe*").await;
        assert_eq!(by_prefix, vec![f.jane]);
        // A prefix never loses a row the bare term found.
        assert_eq!(run(&mut conn, ListKind::Contacts, "doe").await, by_prefix);
        // And the same on a conversation, through the participant's name.
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "doe*").await,
            sorted(vec![f.jane_direct, f.big_group])
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

mod text_words {
    use super::*;

    #[tokio::test]
    async fn name_and_handle_on_contacts() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "name:jane").await,
            vec![f.jane]
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "name:none").await,
            vec![f.nameless]
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "name:any").await,
            sorted(vec![f.ana, f.bo, f.cy, f.jane, f.sam])
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "handle:gmail").await,
            vec![f.jane]
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "handle:+1555*")
                .await
                .len(),
            4
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "handle:none").await,
            Vec::<i64>::new()
        );
    }

    #[tokio::test]
    async fn name_handle_and_title_on_conversations() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "name:jane").await,
            sorted(vec![f.jane_direct, f.big_group])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "handle:icloud").await,
            sorted(vec![f.sam_direct, f.archive_group, f.big_group])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "title:book").await,
            vec![f.big_group]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "title:none").await,
            sorted(vec![f.ana_direct, f.bo_direct, f.jane_direct, f.sam_direct])
        );
    }

    #[tokio::test]
    async fn body_subject_and_filename() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Messages, "body:toast").await,
            vec![f.jane_avocado_to_me]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "body:toast").await,
            vec![f.jane_direct]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "subject:dinner").await,
            vec![f.big_group_msg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "subject:any").await,
            vec![f.big_group_msg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "filename:beach*").await,
            vec![f.feb_big_jpeg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "filename:.jpg").await,
            sorted(vec![f.feb_big_jpeg, f.feb_small_jpeg, f.may_big_jpeg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "filename:notes").await,
            vec![f.jane_direct]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "-body:avocado body:any")
                .await
                .len(),
            11
        );
    }
}

mod people_words {
    use super::*;

    #[tokio::test]
    async fn from_to_and_with() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        // Spec case 4.
        assert_eq!(
            run(
                &mut conn,
                ListKind::Messages,
                r#"from:me to:"Jane Doe" (avocado or "guacamole night")"#
            )
            .await,
            sorted(vec![f.jane_avocado_from_me, f.jane_guac_from_me])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "from:jane").await,
            sorted(vec![
                f.jane_avocado_to_me,
                f.jane_2018,
                f.feb_big_jpeg,
                f.feb_small_jpeg,
                f.feb_pdf,
                f.may_big_jpeg
            ])
        );
        assert_eq!(run(&mut conn, ListKind::Messages, "from:me").await.len(), 4);
        assert_eq!(run(&mut conn, ListKind::Messages, "to:me").await.len(), 10);
        assert_eq!(
            run(&mut conn, ListKind::Messages, "from:gmail.com")
                .await
                .len(),
            6
        );
        assert_eq!(
            run(
                &mut conn,
                ListKind::Conversations,
                &format!("with:#{}", f.jane)
            )
            .await,
            sorted(vec![f.jane_direct, f.big_group])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "with:sam").await,
            sorted(vec![f.sam_direct, f.archive_group, f.big_group])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "with:bo body:old").await,
            vec![f.archive_msg]
        );
        // "Robin" is a participant the source only named: no handle of their
        // own, so only the participant-row leg of `with:` can find them.
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "with:robin").await,
            vec![f.archive_group]
        );
    }

    #[tokio::test]
    async fn in_one_conversation() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(
                &mut conn,
                ListKind::Messages,
                &format!("in:#{}", f.jane_direct)
            )
            .await
            .len(),
            8
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "in:club").await,
            vec![f.big_group_msg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "in:+15550002").await,
            vec![f.bo_2023]
        );
    }

    #[tokio::test]
    async fn contact_groups_on_every_list() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "group:Family").await,
            vec![f.ana]
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "group:family").await,
            vec![f.ana]
        );
        assert_eq!(
            run(
                &mut conn,
                ListKind::Contacts,
                &format!("group:#{}", f.family)
            )
            .await,
            vec![f.ana]
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "group:none").await,
            sorted(vec![f.bo, f.cy, f.jane, f.sam, f.nameless])
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "group:unknown").await,
            vec![f.nameless]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "group:Family").await,
            sorted(vec![f.ana_direct, f.archive_group, f.big_group])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "-group:Family").await,
            sorted(vec![f.bo_direct, f.jane_direct, f.sam_direct])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "group:Family body:hello").await,
            vec![f.ana_2018]
        );
    }

    #[tokio::test]
    async fn message_tags_on_every_list() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "tag:Archive").await,
            vec![f.archive_group]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "tag:none")
                .await
                .len(),
            5
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "tag:Archive").await,
            sorted(vec![f.ana, f.bo, f.sam])
        );
        assert_eq!(
            run(
                &mut conn,
                ListKind::Messages,
                &format!("tag:#{}", f.archive)
            )
            .await,
            vec![f.archive_msg]
        );
    }

    #[tokio::test]
    async fn import_runs() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO vault_imports (account_id, source, mode, status, started_at)
             VALUES ($1, 'imessage', 'push', 'completed', '2024-01-01T00:00:00Z') RETURNING id",
        )
        .bind(ACCOUNT)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::query("UPDATE messages SET import_id = $1 WHERE id = $2")
            .bind(run_id)
            .bind(f.bo_2023)
            .execute(&mut *conn)
            .await
            .unwrap();
        // The duplicate-only message is part of this run too: a search about
        // one Import Run looks at every message it touched, duplicates
        // included, since a re-import is often nothing but duplicates.
        sqlx::query("UPDATE messages SET import_id = $1 WHERE id = $2")
            .bind(run_id)
            .bind(f.dup_only_msg)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Messages, "import:last").await,
            sorted(vec![f.bo_2023, f.dup_only_msg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, &format!("import:#{run_id}")).await,
            sorted(vec![f.bo_2023, f.dup_only_msg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "import:last").await,
            sorted(vec![f.bo_direct, f.dup_only_conv])
        );
        // Without `import:`, the duplicate-only conversation stays hidden:
        // only a search about the run itself looks at it.
        assert!(
            !run(&mut conn, ListKind::Conversations, "")
                .await
                .contains(&f.dup_only_conv)
        );
    }
}

mod kind_words {
    use super::*;

    #[tokio::test]
    async fn kind_service_and_source() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "kind:direct").await,
            sorted(vec![f.ana_direct, f.bo_direct, f.jane_direct, f.sam_direct])
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "kind:group").await,
            sorted(vec![f.ana, f.bo, f.jane, f.sam])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "service:sms").await,
            vec![f.bo_2023]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "service:sms,whatsapp").await,
            sorted(vec![f.bo_2023, f.archive_msg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "service:sms").await,
            vec![f.bo]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "source:whatsapp").await,
            vec![f.archive_msg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "source:imessage")
                .await
                .len(),
            5
        );
    }

    #[tokio::test]
    async fn attachments_by_kind_and_size() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:image").await,
            sorted(vec![f.feb_big_jpeg, f.feb_small_jpeg, f.may_big_jpeg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:pdf").await,
            vec![f.feb_pdf]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:document").await,
            vec![f.feb_pdf]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:video").await,
            Vec::<i64>::new()
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:any")
                .await
                .len(),
            4
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "attachment:none")
                .await
                .len(),
            10
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "attachment:image").await,
            vec![f.jane_direct]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "size:>500k").await,
            sorted(vec![f.feb_big_jpeg, f.feb_pdf, f.may_big_jpeg])
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "size:<500k").await,
            vec![f.feb_small_jpeg]
        );
        assert_eq!(
            run(&mut conn, ListKind::Messages, "size:100k..2M")
                .await
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn trash_is_a_word() {
        let (pool, _dir, f) = seeded().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "trashed:yes").await,
            vec![f.trashed_conv]
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "trashed:no")
                .await
                .len(),
            6
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "trashed:any")
                .await
                .len(),
            7
        );
        assert_eq!(
            run(&mut conn, ListKind::Conversations, "trashed:yes gone").await,
            vec![f.trashed_conv]
        );
        sqlx::query("INSERT INTO trashed_contacts (account_id, contact_id) VALUES ($1, $2)")
            .bind(ACCOUNT)
            .bind(f.cy)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            run(&mut conn, ListKind::Contacts, "trashed:yes").await,
            vec![f.cy]
        );
        assert!(!run(&mut conn, ListKind::Contacts, "").await.contains(&f.cy));
    }
}
