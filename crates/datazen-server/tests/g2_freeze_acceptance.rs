//! WR-G2：上线前冻结验收（web-ready-plan §9 WR-G2 行 / platform-development-plan
//! §11 冻结清单的可码化项）。
//!
//! 冻结原文（平台计划 §11 末段）：「上线前冻结并验收：实际授权变更与
//! permissionVersion 同事务提交；普通 DDL 与历史登记同事务，事务外索引迁移
//! 可核验恢复；schema 支持范围允许旧镜像运行于下一版 expand schema；授权
//! 可见性先于会话存活错误；SSE 缺失 Origin 有明确校验；Web Wapp 使用独立
//! 真实 origin、可嵌入 CSP 与真实 iframe 握手。每项均有对应浏览器/管理库
//! 故障测试，不以 fake 替代。」
//!
//! 本文件只装**管理库故障腿**（真 SQLite 文件 + 真 HTTP oneshot + 真 header
//! 组合）；SSE Origin 的故障腿在 `sse_events.rs`（追加钉子），Wapp 的
//! 浏览器腿在 `packages/web/src/platform/web/__tests__/wappFreezeAcceptance.test.ts`。
//! 装配样式沿用 `crates/datazen-server/tests` 既有约定：文件 `AdminStore`（`transfer_submission.rs`
//! 的 `admin_path` 形状）+ 直调服务 + 第二只 rusqlite 连接直查（等价运维的
//! `sqlite3` 探查）+ 必要处注入**真实存储故障**（见各测试 doc）。
//!
//! ## 冻结项 → 状态账目表（代码即账本）
//!
//! | 冻结项 | 状态 | 钉子位置 |
//! |----|------|----------|
//! | 授权变更与 permissionVersion 同事务 | ✅ 本文件 `*_version_*` / `*_bump_*`；SSE 复核面 `sse_events.rs` |
//! | 禁用成员 = 撤全部会话 + 递增版本一次完成 | ✅ 本文件 `membership_disable_*` / `a_failing_version_bump_*` |
//! | 普通 DDL 与历史登记同事务 | ✅ 本文件 `a_failed_migration_*`；「事务外索引迁移」当前执行器不存在该路径（全部 DDL 在事务内），无需恢复核验——缺口登记见文件尾 |
//! | expand schema 兼容（旧镜像跑下一版） | ✅ 本文件 `migrating_a_v6_library_*` / `v6_era_statements_*` |
//! | 授权可见性先于会话存活错误 | ✅（可码化半边）本文件 `the_organization_boundary_*` / `an_id_addressed_denial_*`；L6 的 409 分支（运行时会话存活/epoch）随 WR-D4 SessionDirectory 接线，见文件尾登记 |
//! | SSE 缺失/伪造 Origin 校验 | ✅ `sse_events.rs` 追加钉子（既有全分支 + 大小写/端口/重复值缺口） |
//! | Wapp 独立 origin / CSP / iframe 握手 | ✅（单测半边）`packages/web .../wappFreezeAcceptance.test.ts`；真浏览器 iframe/nonce/卸载语义与 `event.origin` 等值校验属环境侧/未实现，见 TS 文件头与报告登记 |

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tower::ServiceExt;

use datazen::shared::id::Timestamp;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{
    Action, AuthorizeRequest, Decision, PolicyService, ResourceRef, ResourceType, Role, SubjectType,
};
use datazen_server::storage::login_sessions::{
    LoginSessionMode, LoginSessions, NewLoginSession, RevokeReason, SessionRejection,
};
use datazen_server::storage::migrations::MIGRATIONS;
use datazen_server::storage::{AdminStore, StorageError, ValidateFailure};

// ── 公共装配 ─────────────────────────────────────────────────────────────

const SERVER_SECRET: &[u8] = b"wr-g2-freeze-server-secret-32-bytes!";

/// 文件库路径（同事务/迁移断言需要第二只手直查与故障注入；内存库做不到）。
fn admin_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dz-g2-freeze-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("admin.sqlite")
}

/// 迁移完毕的文件库 + 它的路径（第二只手直查/注入用——必须用同一只句柄，
/// 路径含 uuid，二次调用会得到另一个空库）。
fn open_file_store(tag: &str) -> (Arc<AdminStore>, PathBuf) {
    let path = admin_path(tag);
    let store = Arc::new(AdminStore::open(&path).expect("file-backed admin store"));
    store.migrate().expect("migrations apply");
    (store, path)
}

/// 第二只 rusqlite 连接（直查/注入用；与生产同一份依赖，运维 `sqlite3` 等价）。
fn probe(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).expect("reopen admin store");
    conn.busy_timeout(Duration::from_secs(5))
        .expect("busy timeout");
    conn
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn future_timestamp(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn past_timestamp(hours: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn policy(store: &Arc<AdminStore>) -> PolicyService {
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")
}

fn session_input(org: &str, principal: &str, session_id: &str) -> NewLoginSession {
    NewLoginSession {
        organization_id: org.into(),
        session_id: session_id.into(),
        principal_id: principal.into(),
        auth_time: now_rfc3339(),
        issued_at: now_rfc3339(),
        absolute_expires_at: future_timestamp(24),
        idle_expires_at: future_timestamp(8),
        permission_version: 1,
        client_binding: "ua|client-1".into(),
        mode: LoginSessionMode::Cookie,
        native_secret_hash: None,
    }
}

fn connection_resource(org: &str, id: &str) -> ResourceRef {
    ResourceRef {
        organization_id: org.into(),
        resource_type: ResourceType::Connection,
        resource_id: id.into(),
    }
}

fn authorize_request(
    org: &str,
    principal: &str,
    resource: &ResourceRef,
    action: Action,
) -> AuthorizeRequest {
    AuthorizeRequest {
        organization_id: org.into(),
        principal_id: principal.into(),
        resource: resource.clone(),
        action,
    }
}

// ── 冻结项 1：授权变更与 permissionVersion 同事务（§6.3 / §3.10）─────────

fn every_authorization_entry_point_setup(tag: &str) -> (PolicyService, PathBuf) {
    let (store, path) = open_file_store(tag);
    let service = policy(&store);
    service
        .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
        .expect("membership");
    (service, path)
}

/// 冻结项 1（team-server §6.3 / persistence-model §3.10 / 平台计划 §11）：
/// 「实际授权变更与 permissionVersion 同事务提交」「不存在只改授权不递增
/// 版本的入口」。真实管理库（SQLite 文件）上逐个驱动**每一个**管理入口
/// （grant / deny / revoke / disable / enable），断言授权效果与版本递增
/// **同时**生效、版本单调恰好 +1——这是「同一事务 helper」结构的行为等价
/// 断言（等价断言的登记理由见模块头账目表）。
#[test]
fn every_authorization_entry_point_moves_the_version_exactly_once() {
    let (service, path) = every_authorization_entry_point_setup("entry-points");

    let mut version = service.permission_version("org-a").expect("version");
    assert_eq!(version, 1, "无变更时为初始 1（§3.10 DEFAULT 1）");
    let resource = connection_resource("org-a", "conn-1");

    // grant：授权行生效 ⇄ 版本 +1（同一个提交里同时可见）。
    let change = service
        .grant_acl(
            "org-a",
            &resource,
            SubjectType::User,
            "user-1",
            Action::Write,
        )
        .expect("grant");
    version += 1;
    assert_eq!(change.permission_version, version);
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &resource,
                Action::Write
            ))
            .expect("authorize"),
        Decision::Allowed,
        "授权行与版本同时生效"
    );

    // deny：显式 deny 压过 grant，版本照常 +1。
    let change = service
        .deny_acl(
            "org-a",
            &resource,
            SubjectType::User,
            "user-1",
            Action::Write,
        )
        .expect("deny");
    version += 1;
    assert_eq!(change.permission_version, version);
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &resource,
                Action::Write
            ))
            .expect("authorize"),
        Decision::Denied
    );

    // revoke：移除条目（幂等删除也递增版本），回到基线拒绝。
    let change = service
        .revoke_acl(
            "org-a",
            &resource,
            SubjectType::User,
            "user-1",
            Action::Write,
        )
        .expect("revoke");
    version += 1;
    assert_eq!(change.permission_version, version);
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &resource,
                Action::Write
            ))
            .expect("authorize"),
        Decision::Denied,
        "orgMember 基线不可写"
    );

    // disable / enable：成员状态变更同版本递增一次完成。
    let change = service
        .set_membership_enabled("org-a", "user-1", false, RevokeReason::Admin)
        .expect("disable");
    version += 1;
    assert_eq!(change.permission_version, version);
    let change = service
        .set_membership_enabled("org-a", "user-1", true, RevokeReason::Admin)
        .expect("enable");
    version += 1;
    assert_eq!(change.permission_version, version);

    // 版本行落库与最终授权状态一致（第二只手直查——提交后的持久事实）。
    let conn = probe(&path);
    let stored: i64 = conn
        .query_row(
            "SELECT permission_version FROM permission_versions WHERE organization_id = 'org-a'",
            [],
            |row| row.get(0),
        )
        .expect("version row");
    assert_eq!(stored, version, "落库版本与观测序列一致（单调只增）");
}

fn a_failing_version_bump_setup(tag: &str) -> (PolicyService, LoginSessions, PathBuf) {
    let (store, path) = open_file_store(tag);
    let service = policy(&store);
    service
        .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    (service, sessions, path)
}

/// 冻结项 1 的**故障半边**（真 SQLite 故障注入）：向 `permission_versions`
/// 注入一个 BEFORE UPDATE 触发器使版本递增必然失败（RAISE(ABORT)——真实
/// 存储故障源：运维误装的守护触发器/迁移残留，不是测试替身）。断言：
/// 授权变更（deny 行、成员禁用、会话撤销）**随之一起回滚**——版本递增失败
/// 时不存在「只改授权」的半套状态。这正是 §6.3「同事务提交」的反证面。
#[test]
fn a_failing_version_bump_rolls_back_the_authorization_change_with_it() {
    let (service, sessions, path) = a_failing_version_bump_setup("bump-fault");
    sessions
        .create(session_input("org-a", "user-1", "sess-1"))
        .expect("session");

    let resource = connection_resource("org-a", "conn-1");
    // 先成功一次 grant：版本行落位（INSERT 路径），后续 bump 走 UPDATE 路径。
    service
        .grant_acl(
            "org-a",
            &resource,
            SubjectType::User,
            "user-1",
            Action::Write,
        )
        .expect("first grant");
    let version_before = service.permission_version("org-a").expect("version");
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &resource,
                Action::Write
            ))
            .expect("authorize"),
        Decision::Allowed
    );

    // 注入真实存储故障：任何对 permission_versions 的 UPDATE 都失败。
    {
        let conn = probe(&path);
        conn.execute_batch(
            "CREATE TRIGGER g2_freeze_bump_guard
             BEFORE UPDATE ON permission_versions
             BEGIN SELECT RAISE(ABORT, 'injected: version bump failure'); END;",
        )
        .expect("poison trigger");
    }

    // deny：bump 失败 → 整个事务回滚 → deny 行不存在、原 grant 原样。
    let error = service
        .deny_acl(
            "org-a",
            &resource,
            SubjectType::User,
            "user-1",
            Action::Write,
        )
        .expect_err("poisoned bump must fail the whole use case");
    assert!(
        error.to_string().contains("permission_versions"),
        "失败来自版本递增（而非授权写入本身）：{error}"
    );
    {
        let conn = probe(&path);
        let deny_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM acl_entries
                 WHERE organization_id = 'org-a' AND deny = 1",
                [],
                |row| row.get(0),
            )
            .expect("deny count");
        assert_eq!(deny_rows, 0, "deny 行随失败的事务一起消失");
    }
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &resource,
                Action::Write
            ))
            .expect("authorize"),
        Decision::Allowed,
        "原 grant 不受半套变更污染"
    );
    assert_eq!(
        service.permission_version("org-a").expect("version"),
        version_before,
        "版本未被失败事务推进"
    );

    // disable：禁用 + 撤全部会话 + bump 同事务——bump 失败则三件事都不发生。
    let error = service
        .set_membership_enabled("org-a", "user-1", false, RevokeReason::PermissionRevoked)
        .expect_err("poisoned bump must fail the disable use case");
    assert!(
        error.to_string().contains("permission_versions"),
        "失败来自版本递增：{error}"
    );
    {
        let conn = probe(&path);
        let enabled: i64 = conn
            .query_row(
                "SELECT enabled FROM memberships
                 WHERE organization_id = 'org-a' AND principal_id = 'user-1'",
                [],
                |row| row.get(0),
            )
            .expect("membership enabled flag");
        assert_eq!(enabled, 1, "成员未被禁用（禁用随事务回滚）");
        let revoked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM login_sessions WHERE revoked_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("revoked count");
        assert_eq!(revoked, 0, "会话未被撤销（撤销随事务回滚）");
    }
    let validated = sessions.validate("sess-1", Timestamp::new(now_rfc3339()), 3600);
    assert!(
        validated.is_ok(),
        "撤销在同一事务内被回滚：会话仍然存活（{validated:?}）"
    );
    assert_eq!(
        service.permission_version("org-a").expect("version"),
        version_before
    );
}

fn membership_disable_setup(tag: &str) -> (PolicyService, LoginSessions, PathBuf) {
    let (store, path) = open_file_store(tag);
    let service = policy(&store);
    service
        .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    (service, sessions, path)
}

/// 冻结项 1 的组合面（§4.5 + §6.3 同事务组合 + §3.10 落库事实）：禁用成员
/// 时「UPDATE memberships + 撤销全部会话 + 递增 permission_version」在**一个
/// 提交**里完成且一次递增；撤销原因、token_epoch（§4.5：撤销自增令牌纪元）
/// 与禁用后的授权面（NotVisible）逐项对账——真实文件库 + 第二只手直查。
#[test]
fn membership_disable_revokes_sessions_and_bumps_the_version_in_one_commit() {
    let (service, sessions, path) = membership_disable_setup("disable-tx");
    sessions
        .create(session_input("org-a", "user-1", "sess-1"))
        .expect("session 1");
    sessions
        .create(session_input("org-a", "user-1", "sess-2"))
        .expect("session 2");

    let before = service.permission_version("org-a").expect("version");
    let change = service
        .set_membership_enabled("org-a", "user-1", false, RevokeReason::PermissionRevoked)
        .expect("disable");
    assert_eq!(change.permission_version, before + 1, "版本恰好递增一次");
    assert_eq!(change.revoked_sessions, 2, "两个会话一并撤销");

    // 撤销立即可见（§4.5：撤销后下一个入站请求必 401）。
    assert_eq!(
        sessions.validate("sess-1", Timestamp::new(now_rfc3339()), 3600),
        Err(ValidateFailure::Rejected(SessionRejection::Revoked(
            RevokeReason::PermissionRevoked
        ))),
    );
    assert!(matches!(
        sessions.validate("sess-2", Timestamp::new(now_rfc3339()), 3600),
        Err(ValidateFailure::Rejected(_))
    ));
    // 禁用后授权面：任何资源都不可见（§4.5 的不变量兜底）。
    assert_eq!(
        service
            .authorize(&authorize_request(
                "org-a",
                "user-1",
                &connection_resource("org-a", "conn-1"),
                Action::Read
            ))
            .expect("authorize"),
        Decision::NotVisible
    );

    // 落库对账（第二只手直查提交后的行）。
    let conn = probe(&path);
    let (enabled,): (i64,) = conn
        .query_row(
            "SELECT enabled FROM memberships
             WHERE organization_id = 'org-a' AND principal_id = 'user-1'",
            [],
            |row| Ok((row.get(0)?,)),
        )
        .expect("membership row");
    assert_eq!(enabled, 0);
    let (revoked, reasons, epochs): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(revoke_reason = 'permission_revoked'), 0),
                    COALESCE(SUM(token_epoch = 2), 0)
             FROM login_sessions
             WHERE organization_id = 'org-a' AND session_id IN ('sess-1', 'sess-2')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("session rows");
    assert_eq!(revoked, 2, "两行都带 revoked_at");
    assert_eq!(
        reasons, 2,
        "撤销原因 = permission_revoked（§4.3 CHECK 词表）"
    );
    assert_eq!(epochs, 2, "token_epoch 各自增一次（§4.5）");
    let stored_version: i64 = conn
        .query_row(
            "SELECT permission_version FROM permission_versions WHERE organization_id = 'org-a'",
            [],
            |row| row.get(0),
        )
        .expect("version row");
    assert_eq!(stored_version, before + 1);
}

// ── 冻结项 2：DDL 与历史登记同事务 + expand schema 兼容（§11 / §13）──────

/// 以 v6 形态手工建库（v1–v6 的 expand SQL + 版本登记行 + 历史行）——旧镜像
/// 留下的真实前状态的复刻。返回（路径，迁移前的 sqlite_master 快照）。
fn v6_form_library(tag: &str) -> (PathBuf, BTreeMap<(String, String), String>) {
    let path = admin_path(tag);
    {
        let conn = Connection::open(&path).expect("create v6 library");
        conn.busy_timeout(Duration::from_secs(5))
            .expect("busy timeout");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS admin_schema_version (
               version INTEGER PRIMARY KEY,
               name TEXT NOT NULL,
               applied_at TEXT NOT NULL
             );",
        )
        .expect("version table");
        for migration in &MIGRATIONS[..6] {
            conn.execute_batch(migration.expand)
                .unwrap_or_else(|error| panic!("v{} expand: {error}", migration.version));
            conn.execute(
                "INSERT INTO admin_schema_version (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    migration.version,
                    migration.name,
                    "2025-12-01T00:00:00.000Z"
                ],
            )
            .expect("version row");
        }

        // 历史行（v1–v6 各登记面的真实数据形状；「历史登记」= 审计/账本行）。
        conn.execute_batch(
            r#"
            INSERT INTO connection_profile (organization_id, connection_id, name, driver_id,
                config_revision, credential_revision, initial_namespace_json,
                created_at, updated_at)
            VALUES ('org-a', 'conn-1', 'primary', 'postgres', 1, 1, '[]',
                    '2025-12-01T00:00:00.000Z', '2025-12-01T00:00:00.000Z');

            INSERT INTO known_hosts VALUES
                ('org-a', 'db.internal:5432', 'SHA256:aa', 'ssh-ed25519', 'AAAKEY');

            INSERT INTO memberships VALUES
                ('org-a', 'user-1', 'orgMember', 'idp:u1', 1,
                 '2025-12-01T00:00:00.000Z', '2025-12-01T00:00:00.000Z');

            INSERT INTO login_sessions (organization_id, session_id, principal_id, auth_time,
                issued_at, absolute_expires_at, idle_expires_at, permission_version,
                client_binding)
            VALUES ('org-a', 'sess-1', 'user-1', '2025-12-01T00:00:00.000Z',
                    '2025-12-01T00:00:00.000Z', '2026-01-02T00:00:00.000Z',
                    '2026-01-01T08:00:00.000Z', 1, 'ua|client-1');

            INSERT INTO acl_entries VALUES
                ('org-a', 'connection', 'conn-1', 'user', 'user-1', 'write', 0,
                 '2025-12-01T00:00:00.000Z');

            INSERT INTO permission_versions VALUES
                ('org-a', 4, '2025-12-01T00:00:00.000Z');

            INSERT INTO audit_events (organization_id, occurred_at, principal_id, principal_kind,
                action, connection_id, outcome, request_id)
            VALUES ('org-a', '2025-12-01T00:00:00.000Z', 'user-1', 'user',
                    'connection.created', 'conn-1', 'succeeded', 'req-1');
            "#,
        )
        .expect("history rows");
    }
    let conn = Connection::open(&path).expect("snapshot connection");
    (path, schema_snapshot(&conn))
}

/// sqlite_master 快照（自动索引 sql 为 NULL，用 COALESCE 归一）。
fn schema_snapshot(conn: &Connection) -> BTreeMap<(String, String), String> {
    conn.prepare(
        "SELECT type, name, COALESCE(sql, '')
         FROM sqlite_master
         WHERE name NOT LIKE 'sqlite_%'
         ORDER BY type, name",
    )
    .expect("master query")
    .query_map([], |row| {
        Ok((
            (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
            row.get::<_, String>(2)?,
        ))
    })
    .expect("master map")
    .collect::<Result<BTreeMap<_, _>, _>>()
    .expect("master rows")
}

/// 冻结项 2（平台计划 §11「schema 支持范围允许旧镜像运行于下一版 expand
/// schema」/ storage/mod.rs 的 expand 纪律）：v6 形态旧库（真实 SQLite 文件、
/// 带历史行）跑 `migrate()` → 只落 v7..=v15 的追加步；**每一个**既有对象（表/索引）的
/// sqlite_master DDL 文本逐字节不变（expand-only：旧列未删未改类型），全部
/// 历史行原样，v7 六表就位，版本登记到 15。
#[test]
fn migrating_a_v6_library_expands_and_preserves_every_history_row() {
    let (path, before) = v6_form_library("v6-expand");
    let store = AdminStore::open(&path).expect("open v6 library");
    let applied = store.migrate().expect("migrate v6 → 当前目标（v15）");
    // v8（server_settings）、v9（named_views）、v10（org_memories /
    // org_memory_audit，T25）、v11（query_favorites / query_history，
    // W8 刀3a；编号让位于 T25）、v12（job 运行时三伴表 + 迁移历史 +
    // schema_diff_profiles，W8 刀3b）、v13（workflows + 运行历史，
    // W8 刀3c）、v14（dashboard 五表，W8 刀3d）与 v15（sync 双表，
    // W8 刀3e-2）加入后，v6 旧库一次应用 v7..=v15 九步——expand-only
    // 纪律对每一步分别成立。
    assert_eq!(
        applied.len(),
        9,
        "v6 → 当前目标应用 v7、v8、v9、v10、v11、v12、v13、v14、v15 九步"
    );
    assert_eq!(applied[0].version, 7);
    assert_eq!(applied[1].version, 8);
    assert_eq!(applied[2].version, 9);
    assert_eq!(applied[3].version, 10);
    assert_eq!(applied[4].version, 11);
    assert_eq!(applied[5].version, 12);
    assert_eq!(applied[6].version, 13);
    assert_eq!(applied[7].version, 14);
    assert_eq!(applied[8].version, 15);
    assert_eq!(store.schema_version().expect("schema version"), 15);

    let conn = probe(&path);
    // v7 新表在（执行接线的四组对象）。
    for table in [
        "stored_secret",
        "stored_secret_version",
        "connection_catalog",
        "connection_catalog_group",
        "tunnel_definition",
        "execution_service_identity",
        "org_memories",
        "org_memory_audit",
        "query_favorites",
        "query_history",
        "job_result_details",
        "job_domain_results",
        "job_artifact_refs",
        "migration_run_history",
        "schema_diff_profiles",
        "workflows",
        "workflow_run_history",
        "dashboards",
        "widgets",
        "widget_runs",
        "widget_latest_run",
        "monitor_schedule_grants",
        // v15（W8 刀3e-2 data sync：任务断点 + 复用档案）。
        "sync_profiles",
        "sync_tasks",
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .expect("table probe");
        assert!(exists, "v7 表 {table} 必须存在");
    }

    // expand-only：迁移前存在的每个对象的 DDL 文本逐字节不变。
    let after = schema_snapshot(&conn);
    for (key, sql) in &before {
        assert_eq!(
            after.get(key),
            Some(sql),
            "既有对象 {key:?} 的 DDL 被 v7 改写（违反 expand-only）"
        );
    }
    // 新增对象 = v7 的 9 个 + v8..=v15 的逐刀追加（自动索引不入快照）。
    let added: Vec<_> = after
        .keys()
        .filter(|key| !before.contains_key(key))
        .cloned()
        .collect();
    assert_eq!(
        added,
        vec![
            ("index", "idx_connection_catalog_order"),
            ("index", "idx_connection_catalog_tunnel"),
            // v12（W8 刀3b schema diff 运行时 + 迁移历史 + profile）。
            ("index", "idx_job_artifact_expiry"),
            ("index", "idx_migration_run_profile"),
            ("index", "idx_migration_run_started"),
            // v14（W8 刀3d dashboard 运行时：monitor 授权单）。
            ("index", "idx_monitor_grant_widget"),
            ("index", "idx_named_views_scope"),
            // v10（T25 组织记忆与审批审计）。
            ("index", "idx_org_memories_active"),
            ("index", "idx_org_memory_audit_org"),
            // v11（query assets，W8 刀3a 收藏 + 查询历史的服务端存储）。
            ("index", "idx_query_favorites_owner"),
            ("index", "idx_query_history_owner"),
            // v8（server_settings，WR-C2 §6.1 settings 域的服务端存储）。
            ("index", "idx_server_settings_org"),
            // v15（W8 刀3e-2 data sync：档案连接对 + 任务状态观测）。
            ("index", "idx_sync_profiles_connections"),
            ("index", "idx_sync_tasks_status"),
            // v14（W8 刀3d）。
            ("index", "idx_widget_runs_widget_started"),
            ("index", "idx_widgets_dashboard"),
            ("index", "idx_widgets_workflow"),
            // v13（W8 刀3c workflow 运行时：定义 + 运行历史）。
            ("index", "idx_workflow_run_history_flow"),
            ("index", "idx_workflows_visibility"),
            ("index", "uq_execution_service_identity_principal"),
            ("table", "connection_catalog"),
            ("table", "connection_catalog_group"),
            // v14（W8 刀3d）。
            ("table", "dashboards"),
            ("table", "execution_service_identity"),
            // v12（W8 刀3b）。
            ("table", "job_artifact_refs"),
            ("table", "job_domain_results"),
            ("table", "job_result_details"),
            ("table", "migration_run_history"),
            // v14（W8 刀3d monitor 授权单）。
            ("table", "monitor_schedule_grants"),
            ("table", "named_views"),
            // v10（T25 组织记忆与审批审计）。
            ("table", "org_memories"),
            ("table", "org_memory_audit"),
            // v11（query assets，W8 刀3a）。
            ("table", "query_favorites"),
            ("table", "query_history"),
            ("table", "schema_diff_profiles"),
            ("table", "server_settings"),
            ("table", "stored_secret"),
            ("table", "stored_secret_version"),
            // v15（W8 刀3e-2 data sync）。
            ("table", "sync_profiles"),
            ("table", "sync_tasks"),
            ("table", "tunnel_definition"),
            // v14（W8 刀3d）。
            ("table", "widget_latest_run"),
            ("table", "widget_runs"),
            ("table", "widgets"),
            // v13（W8 刀3c）。
            ("table", "workflow_run_history"),
            ("table", "workflows"),
        ]
        .into_iter()
        .map(|(t, n): (&str, &str)| (t.to_string(), n.to_string()))
        .collect::<Vec<_>>(),
        "新增对象恰为 v7..=v15 的登记清单（expand-only：每一步只加不改）"
    );

    // 历史行逐字段原样（登记表/审计行不被迁移破坏）。
    let (name, driver, config_rev): (String, String, i64) = conn
        .query_row(
            "SELECT name, driver_id, config_revision FROM connection_profile
             WHERE organization_id = 'org-a' AND connection_id = 'conn-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("profile row");
    assert_eq!(
        (name.as_str(), driver.as_str(), config_rev),
        ("primary", "postgres", 1)
    );
    let (role, enabled): (String, i64) = conn
        .query_row(
            "SELECT role_id, enabled FROM memberships
             WHERE organization_id = 'org-a' AND principal_id = 'user-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("membership row");
    assert_eq!((role.as_str(), enabled), ("orgMember", 1));
    let (permission_version, revoked): (i64, i64) = conn
        .query_row(
            "SELECT permission_version, (CASE WHEN revoked_at IS NULL THEN 0 ELSE 1 END)
             FROM login_sessions
             WHERE organization_id = 'org-a' AND session_id = 'sess-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("session row");
    assert_eq!((permission_version, revoked), (1, 0));
    let (deny, action): (i64, String) = conn
        .query_row(
            "SELECT deny, action FROM acl_entries
             WHERE organization_id = 'org-a' AND resource_id = 'conn-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("acl row");
    assert_eq!((deny, action.as_str()), (0, "write"));
    let stored_version: i64 = conn
        .query_row(
            "SELECT permission_version FROM permission_versions WHERE organization_id = 'org-a'",
            [],
            |row| row.get(0),
        )
        .expect("version row");
    assert_eq!(stored_version, 4, "权限版本历史值不被迁移触碰");
    let (audit_action, outcome, payload): (String, String, String) = conn
        .query_row(
            "SELECT action, outcome, payload_json FROM audit_events
             WHERE organization_id = 'org-a' AND event_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("audit row");
    assert_eq!(
        (audit_action.as_str(), outcome.as_str(), payload.as_str()),
        ("connection.created", "succeeded", "{}")
    );
}

fn v6_era_statements_setup(tag: &str) -> PathBuf {
    let (store, path) = open_file_store(tag);
    store.migrate().expect("full migrate to v7");
    path
}

/// 冻结项 2 的「旧镜像运行于下一版」半边：v1–v7 全量展开后，**旧镜像的
/// 访问路径**（只认识 v1–v6 列清单的语句）在 expand schema 上照常读写——
/// 迁移没有删列、没有改名、没有改类型，旧语句一个字不改即可执行。
#[test]
fn v6_era_statements_still_run_on_the_expanded_schema() {
    let path = v6_era_statements_setup("old-image");
    let conn = probe(&path);

    // v1 时代的 connection_profile 列清单写入 + 读回。
    conn.execute(
        "INSERT INTO connection_profile (organization_id, connection_id, name, driver_id,
            config_revision, credential_revision, initial_namespace_json,
            created_at, updated_at)
         VALUES ('org-a', 'conn-2', 'second', 'sqlite', 1, 1, '[]',
                 '2025-12-02T00:00:00.000Z', '2025-12-02T00:00:00.000Z')",
        [],
    )
    .expect("v1-era insert on the expanded schema");
    let name: String = conn
        .query_row(
            "SELECT name FROM connection_profile
             WHERE organization_id = 'org-a' AND connection_id = 'conn-2'",
            [],
            |row| row.get(0),
        )
        .expect("v1-era select");
    assert_eq!(name, "second");

    // v6 时代的审计登记（append-only 历史）照常追加。
    conn.execute(
        "INSERT INTO audit_events (organization_id, occurred_at, principal_kind, action, outcome)
         VALUES ('org-a', '2025-12-02T00:00:00.000Z', 'user', 'profile.created', 'succeeded')",
        [],
    )
    .expect("v6-era audit append");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE organization_id = 'org-a'",
            [],
            |row| row.get(0),
        )
        .expect("audit count");
    assert_eq!(count, 1);
}

/// 冻结项 2 的**故障半边**（真 SQLite 故障注入）：v6 旧库里预置一个与 v7
/// 冲突的同名表（真实故障源：上一次回滚留下的遗留对象）→ v7 的 CREATE
/// TABLE 失败 → 断言**整个迁移原子回滚**：版本登记行（历史登记）不存在、
/// 已执行的前半 DDL（stored_secret 等）不存在、历史行原样、库仍停在 v6。
/// 「DDL 与版本登记同事务」的反证面。
#[test]
fn a_failed_migration_rolls_back_ddl_and_its_version_row_together() {
    let (path, before) = v6_form_library("v6-conflict");
    {
        let conn = Connection::open(&path).expect("poison connection");
        conn.busy_timeout(Duration::from_secs(5))
            .expect("busy timeout");
        conn.execute_batch("CREATE TABLE connection_catalog (organization_id TEXT NOT NULL);")
            .expect("poison table");
    }
    let store = AdminStore::open(&path).expect("open poisoned library");
    let error = store
        .migrate()
        .expect_err("v7 conflicts with the legacy table");
    assert!(
        matches!(error, StorageError::Migration { version: 7, .. }),
        "失败归因到 v7：{error}"
    );

    let conn = probe(&path);
    let registered: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM admin_schema_version WHERE version = 7",
            [],
            |row| row.get(0),
        )
        .expect("version row probe");
    assert_eq!(registered, 0, "版本登记行与 DDL 同事务：失败即一起消失");
    let stored_secret_exists: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'stored_secret')",
            [],
            |row| row.get(0),
        )
        .expect("stored_secret probe");
    assert!(
        !stored_secret_exists,
        "v7 前半已执行的 DDL（stored_secret）也被回滚"
    );
    let after = schema_snapshot(&conn);
    for (key, sql) in &before {
        assert_eq!(after.get(key), Some(sql), "历史对象 {key:?} 被失败迁移破坏");
    }
    let history: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("audit count");
    assert_eq!(history, 1, "审计历史行原样");
    assert_eq!(
        store.schema_version().expect("schema version"),
        6,
        "库仍停在 v6"
    );
}

/// 冻结项 2 的幂等半边：带历史行的真实文件库上重跑 `migrate()` → 不应用
/// 任何迁移、不炸、sqlite_master 与历史行纹丝不动。
#[test]
fn migrate_is_idempotent_over_a_history_bearing_library() {
    let (path, _) = v6_form_library("v6-idempotent");
    let store = AdminStore::open(&path).expect("open");
    let first = store.migrate().expect("first migrate");
    assert_eq!(first.len(), 9, "v6 → 当前目标（v7..=v15 九步）");
    let snapshot_after_first = schema_snapshot(&probe(&path));

    let second = store.migrate().expect("second migrate");
    assert!(second.is_empty(), "重跑不应用任何迁移");
    let snapshot_after_second = schema_snapshot(&probe(&path));
    assert_eq!(
        snapshot_after_first, snapshot_after_second,
        "重跑后 schema 纹丝不动"
    );
    let history: i64 = probe(&path)
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("audit count");
    assert_eq!(history, 1, "历史行不受重跑影响");
}

// ── 冻结项 3：授权可见性先于会话存活错误（§3.2 / §6.1 / §7.1）────────────

fn the_organization_boundary_setup(tag: &str) -> (PolicyService, PathBuf) {
    let (store, path) = open_file_store(tag);
    let service = policy(&store);
    service
        .create_membership("org-a", "user-1", Role::OrgMember, "idp:u1")
        .expect("membership");
    (service, path)
}

/// 冻结项 3 的求值顺序半边（§6.1 第 1 步 + authorize.rs 模块头「组织边界在
/// DB 之前判定」）：**真实存储故障**（第二只连接 DROP 掉授权表——管理库
/// 被摧毁的真实故障源）之下，跨组织资源仍精确回答 `NotVisible`（组织边界
/// 不经过数据库），同组织资源如实报存储错误——两相对照证明「可见性判定
/// 先于任何存储访问」，跨组织探测无法借存储故障改判或泄密。
#[test]
fn the_organization_boundary_is_decided_before_any_storage_query() {
    let (service, path) = the_organization_boundary_setup("boundary-order");

    // 注入真实存储故障：授权表整体被摧毁。
    {
        let conn = probe(&path);
        conn.execute_batch("DROP TABLE acl_entries; DROP TABLE memberships;")
            .expect("storage fault");
    }

    // 跨组织：不经存储即可判定 → Ok(NotVisible)。
    let cross_org = service
        .authorize(&authorize_request(
            "org-a",
            "user-1",
            &connection_resource("org-b", "conn-1"),
            Action::Read,
        ))
        .expect("cross-org decision must not touch the destroyed storage");
    assert_eq!(
        cross_org,
        Decision::NotVisible,
        "组织边界先于存储查询：NotVisible 在表被摧毁时依然成立"
    );

    // 同组织：需要读库 → 如实报存储错误（对照证明上面的 NotVisible 不是
    // 「碰巧查到空」而是根本没查）。
    let same_org = service.authorize(&authorize_request(
        "org-a",
        "user-1",
        &connection_resource("org-a", "conn-1"),
        Action::Read,
    ));
    assert!(same_org.is_err(), "同组织判定需要存储：必须如实失败");
}

async fn an_id_addressed_denial_setup(tag: &str) -> (Arc<AdminStore>, Arc<PolicyService>, PathBuf) {
    let (store, path) = open_file_store(tag);
    let policy = Arc::new(policy(&store));
    policy
        .create_membership("org-a", "ops-1", Role::JobOperator, "idp:o1")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    sessions
        .create(session_input("org-a", "ops-1", "sess-live"))
        .expect("live session");
    // 过期会话：绝对窗在过去（L2 的拒绝分支；idle <= absolute 的 CHECK 仍成立）。
    let expired = NewLoginSession {
        absolute_expires_at: past_timestamp(1),
        idle_expires_at: past_timestamp(2),
        ..session_input("org-a", "ops-1", "sess-dead")
    };
    sessions.create(expired).expect("dead session");
    (store, policy, path)
}

/// 冻结项 3 的 HTTP 半边（§7.1 映射 + §3.2 L2/L6 失败行为的分层）：真实
/// 认证链（`build_router_with_auth` + 真登录会话行 + 真 cookie/CSRF 头）
/// oneshot 驱动——
///
/// - 存活会话 + 资源动作被拒（id 寻址）→ **404 `NotFound` + §7.1 固定文案**
///   （与「不存在」完全同形；不是 403，也不是任何会话存活类错误码）；
/// - 自己的过期会话 → **401 `Unauthenticated`**（登录会话存活错误只属于
///   L2，绝不混进 L6 的资源响应——「先撞 404 才对」的防探测顺序由这两层
///   分立保证）；
/// - 对照组：同一存活会话的允许动作穿过 L6 到 fallback（501），证明 404
///   确由 L6 授权拒绝产生。
///
/// 跨组织资源经 HTTP 不可寻址是 L6 的构造事实（resource ref 以会话组织
/// 盖章，authorize.rs L79-88）；跨组织半边由
/// `the_organization_boundary_is_decided_before_any_storage_query` 在 policy
/// 层钉住。L6 的 409 分支（已授权 owner 的运行时会话存活/epoch 复核）随
/// WR-D4 接线，见模块头账目表登记。
#[tokio::test]
async fn an_id_addressed_denial_is_404_not_found_while_a_dead_session_is_401() {
    let (store, policy, _path) = an_id_addressed_denial_setup("http-order").await;

    let router = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: policy.clone(),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        wapp_assets: None,
        domain_routers: None,
    })
    .expect("router");
    let mut app = router;

    // 存活会话 + JobOperator 无 connection:write 基线 → L6 id 寻址拒绝 → 404。
    let csrf_secret = CsrfSecret::new(SERVER_SECRET.to_vec()).expect("csrf secret");
    let token = csrf::mint(&csrf_secret, "sess-live").expect("mint csrf token");
    let denied = HttpRequest::builder()
        .method("PATCH")
        .uri("/api/v1/connections/conn-1")
        .header(
            header::COOKIE,
            format!("__Host-dz_session=sess-live; __Host-dz_csrf={token}"),
        )
        .header("x-dz-csrf-token", token.as_str())
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(denied).await.expect("call");
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "拒绝映射 404（不是 403）"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        body["code"], "notFound",
        "§7.1：code 与「不存在」完全一致（wire 词表 camelCase）"
    );
    assert_eq!(
        body["message"], NOT_VISIBLE_MESSAGE,
        "固定文案：不含资源名、类型、归属（防 ID 枚举）"
    );

    // 对照组：同一存活会话的允许动作（connection:read 基线放行）穿过 L6 →
    // 落到未点亮路由的 fallback 501，证明上面的 404 由 L6 产生。
    let allowed = HttpRequest::builder()
        .method("GET")
        .uri("/api/v1/connections/conn-1")
        .header(header::COOKIE, "__Host-dz_session=sess-live")
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(allowed).await.expect("call");
    assert_eq!(
        response.status(),
        StatusCode::NOT_IMPLEMENTED,
        "L6 放行 → 未点亮路由的 fallback 501"
    );

    // 过期会话 → 401（L2 的存活错误；绝不以 404/403 的资源语义出现）。
    let dead = HttpRequest::builder()
        .method("GET")
        .uri("/api/v1/connections/conn-1")
        .header(header::COOKIE, "__Host-dz_session=sess-dead")
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(dead).await.expect("call");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["code"], "unauthenticated", "wire 词表 camelCase");
}
