//! `LoginSessions` 行为钉子：会话行的三动词（create / validate / revoke）
//! 与 §4.3/§4.5 的语义对齐——绝对/空闲过期、touch 顺延封顶、撤销自增
//! `token_epoch`、外键要求 membership 存在。

use std::sync::Arc;

use datazen::shared::id::Timestamp;

use super::login_sessions::{
    LoginSessionMode, LoginSessions, NewLoginSession, RevokeOutcome, RevokeReason,
    SessionRejection, ValidateFailure,
};
use super::AdminStore;

const NOW: &str = "2026-01-01T00:00:00.000Z";
const ABSOLUTE: &str = "2026-01-02T00:00:00.000Z"; // +24h
const IDLE: &str = "2026-01-01T08:00:00.000Z"; // +8h

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn now() -> Timestamp {
    Timestamp::new(NOW)
}

/// memberships 行是 login_sessions 的外键目标；测试经由 PolicyService 的
/// 管理用例落行（同一 SQL 面），不绕过外键语义。
fn seed_membership(store: &AdminStore, org: &str, principal: &str) {
    let conn = store.lock_for_adapter().expect("lock");
    conn.execute(
        "INSERT INTO memberships (organization_id, principal_id, role_id, external_subject,
                                  created_at, updated_at)
         VALUES (?1, ?2, 'orgMember', ?3, ?4, ?4)",
        rusqlite::params![org, principal, format!("idp:{principal}"), NOW],
    )
    .expect("membership inserts");
}

fn sessions(store: Arc<AdminStore>) -> LoginSessions {
    LoginSessions::new(store)
}

fn new_session(org: &str, principal: &str, session_id: &str) -> NewLoginSession {
    NewLoginSession {
        organization_id: org.into(),
        session_id: session_id.into(),
        principal_id: principal.into(),
        auth_time: NOW.into(),
        issued_at: NOW.into(),
        absolute_expires_at: ABSOLUTE.into(),
        idle_expires_at: IDLE.into(),
        permission_version: 1,
        client_binding: "ua-digest|client-1".into(),
        mode: LoginSessionMode::Cookie,
        native_secret_hash: None,
    }
}

#[test]
fn create_persists_the_row_and_canonicalizes_timestamps() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());

    // 非定宽时刻（秒精度 + 偏移）进边界，出边界必须是定宽 RFC3339 UTC。
    let mut input = new_session("org-a", "user-1", "sid-1");
    input.issued_at = "2026-01-01T01:00:00+01:00".into();
    let row = sessions.create(input).expect("create");

    assert_eq!(row.issued_at, "2026-01-01T00:00:00.000Z");
    assert_eq!(row.token_epoch, 1);
    assert!(!row.reauth_required);
    assert_eq!(row.mode, LoginSessionMode::Cookie);
    assert!(row.native_secret_hash.is_none());
    assert!(row.revoked_at.is_none());
}

#[test]
fn create_requires_the_membership_row_to_exist() {
    let store = store();
    let sessions = sessions(store.clone());
    let error = sessions
        .create(new_session("org-a", "ghost", "sid-x"))
        .expect_err("FK target must exist");
    let text = error.to_string();
    assert!(
        text.contains("membership (org-a, ghost) does not exist"),
        "error must name the missing membership: {text}"
    );
}

#[test]
fn create_rejects_an_idle_window_past_the_absolute_window() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());

    let mut input = new_session("org-a", "user-1", "sid-bad");
    input.idle_expires_at = "2026-01-02T08:00:00.000Z".into(); // 越过 absolute
    let error = sessions.create(input).expect_err("idle must not pass absolute");
    assert!(error.to_string().contains("ck_login_window"));
}

#[test]
fn create_rejects_native_without_secret_hash_and_cookie_with_one() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());

    let mut native = new_session("org-a", "user-1", "sid-n");
    native.mode = LoginSessionMode::Native;
    let error = sessions
        .create(native)
        .expect_err("native without hash violates ck_login_native");
    assert!(error.to_string().contains("ck_login_native"));

    let mut cookie = new_session("org-a", "user-1", "sid-c");
    cookie.native_secret_hash = Some("hmac:1:deadbeef".into());
    let error = sessions
        .create(cookie)
        .expect_err("cookie with hash violates ck_login_native");
    assert!(error.to_string().contains("ck_login_native"));
}

#[test]
fn validate_accepts_a_live_session_and_extends_the_idle_window() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());
    sessions
        .create(new_session("org-a", "user-1", "sid-live"))
        .expect("create");

    let row = sessions
        .validate("sid-live", now(), 8 * 3600)
        .expect("live session validates");
    // touch：空闲窗顺延到 now + 8h（此处与原值同刻，形状不变但值被重写）。
    assert_eq!(row.idle_expires_at, "2026-01-01T08:00:00.000Z");
    assert!(row.revoked_at.is_none());
}

#[test]
fn validate_extends_the_idle_window_and_caps_it_at_the_absolute_window() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());
    sessions
        .create(new_session("org-a", "user-1", "sid-cap"))
        .expect("create");

    // now + 12h 已越过 absolute（+24h 之内？不——now+12h < absolute；改用
    // 一例越顶：原 idle = now+8h，touch 12h → now+12h；absolute = now+24h，
    // 不越顶。真正的封顶路径：把 absolute 收紧到 now+10h。
    let conn = store.lock_for_adapter().expect("lock");
    conn.execute(
        "UPDATE login_sessions SET absolute_expires_at = '2026-01-01T10:00:00.000Z'
         WHERE session_id = 'sid-cap'",
        [],
    )
    .expect("tighten absolute");
    drop(conn);

    let row = sessions
        .validate("sid-cap", now(), 12 * 3600)
        .expect("still live");
    assert_eq!(
        row.idle_expires_at, "2026-01-01T10:00:00.000Z",
        "idle extension must cap at absolute_expires_at (ck_login_window)"
    );
}

#[test]
fn validate_rejects_unknown_revoked_and_expired_sessions() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());
    sessions
        .create(new_session("org-a", "user-1", "sid-revoke"))
        .expect("create revoke");
    sessions
        .create(new_session("org-a", "user-1", "sid-idle"))
        .expect("create idle");
    sessions
        .create(new_session("org-a", "user-1", "sid-absolute"))
        .expect("create absolute");

    assert_eq!(
        sessions.validate("sid-ghost", now(), 3600),
        Err(ValidateFailure::Rejected(SessionRejection::Unknown)),
    );

    sessions
        .revoke("sid-revoke", RevokeReason::Logout, now())
        .expect("revoke");
    assert_eq!(
        sessions.validate("sid-revoke", now(), 3600),
        Err(ValidateFailure::Rejected(SessionRejection::Revoked(
            RevokeReason::Logout
        ))),
    );

    // 空闲过期：now 已越过 idle（+8h）。
    let later = Timestamp::new("2026-01-01T09:00:00.000Z");
    assert_eq!(
        sessions.validate("sid-idle", later, 3600),
        Err(ValidateFailure::Rejected(SessionRejection::IdleExpired)),
    );

    // 绝对过期：now 已越过 absolute（+24h）——绝对窗优先于空闲窗报告。
    let much_later = Timestamp::new("2026-01-03T00:00:00.000Z");
    assert_eq!(
        sessions.validate("sid-absolute", much_later, 3600),
        Err(ValidateFailure::Rejected(SessionRejection::AbsoluteExpired)),
    );
}

#[test]
fn revoke_marks_the_row_and_bumps_the_token_epoch_exactly_once() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    let sessions = sessions(store.clone());
    sessions
        .create(new_session("org-a", "user-1", "sid-1"))
        .expect("create");

    assert_eq!(
        sessions.revoke("sid-1", RevokeReason::PermissionRevoked, now()),
        Ok(RevokeOutcome::Revoked),
    );
    assert_eq!(
        sessions.revoke("sid-1", RevokeReason::PermissionRevoked, now()),
        Ok(RevokeOutcome::AlreadyRevoked),
        "revoke is idempotent: the epoch must not double-bump"
    );
    assert_eq!(
        sessions.revoke("sid-ghost", RevokeReason::Admin, now()),
        Ok(RevokeOutcome::Unknown),
    );

    let conn = store.lock_for_adapter().expect("lock");
    let (revoked_at, reason, epoch) = conn
        .query_row(
            "SELECT revoked_at, revoke_reason, token_epoch FROM login_sessions
             WHERE session_id = 'sid-1'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
        )
        .expect("row");
    drop(conn);
    assert_eq!(revoked_at, NOW);
    assert_eq!(reason, "permission_revoked");
    assert_eq!(epoch, 2, "revoke bumps token_epoch once");
}

#[test]
fn revoke_all_for_principal_only_touches_live_rows_of_that_principal() {
    let store = store();
    seed_membership(&store, "org-a", "user-1");
    seed_membership(&store, "org-a", "user-2");
    seed_membership(&store, "org-b", "user-1");
    let sessions = sessions(store.clone());
    for (org, principal, sid) in [
        ("org-a", "user-1", "s-a1"),
        ("org-a", "user-2", "s-a2"),
        ("org-b", "user-1", "s-b1"),
    ] {
        sessions.create(new_session(org, principal, sid)).expect("create");
    }
    // 先撤销一个，验证幂等不重复计入。
    sessions.revoke("s-a2", RevokeReason::Logout, now()).expect("revoke");

    let revoked = sessions
        .revoke_all_for_principal("org-a", "user-1", RevokeReason::Admin, now())
        .expect("revoke all");
    assert_eq!(revoked, 1, "only org-a/user-1's live row is affected");
    assert_eq!(
        sessions.validate("s-a1", now(), 3600),
        Err(ValidateFailure::Rejected(SessionRejection::Revoked(
            RevokeReason::Admin
        ))),
    );
    // 同 principal 在别的组织不受影响；其他 principal 也不受影响。
    assert!(sessions.validate("s-b1", now(), 3600).is_ok());
    assert!(sessions.validate("s-a2", now(), 3600).is_err());
}
