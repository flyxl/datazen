//! 桌面本地组织 id 统一（`AppDb` v5 迁移）的回归用例。
//!
//! # 背景
//!
//! 桌面的**规范**本地组织 id 是 `crate::platform::LOCAL_ORGANIZATION_ID`
//! （`"datazen-desktop-local"`，定义在 `platform/identity.rs`）。但 2026-10-09
//! 引入 `jobs` 表的那次提交里，迁移三件套（schema-diff / data-transfer / data-sync）
//! 与启动期恢复扫描把**另一个**字面量 `"datazen-local"` 写死成了组织 id。
//!
//! 两个值落在 `jobs.organization_id` 的**同一列**上 ⇒ 同一张表里落着两套组织。
//! 落库组织取自**受理方 ctx**（`jobs/admission.rs`），列举按 `ctx.organization_id`
//! **且** `ctx.principal_id` 过滤（`jobs/access.rs`）⇒ 两类任务互相看不见。
//!
//! v5 迁移把旧行并回规范值。本文件钉住三件事：
//!
//! 1. 旧行的组织确实被改写成规范值，且改写后能被**规范 ctx** 列举到；
//! 2. 撞 `UNIQUE (organization_id, consumed_plan_id)` 的行**不会**让迁移失败
//!    （迁移事务失败 ⇒ **应用起不来**，那是最坏的结果）；
//! 3. 迁移**只**动组织维度——主体维度（`owner_principal_id`）逐字不动。
//!
//! # 为什么可以「造一个 v4 库」
//!
//! v5 是**纯数据**迁移（只 UPDATE，不含任何 DDL），因此「v5 建出来的表结构」与
//! 「v4 建出来的表结构」逐字相同。故本文件的做法是：先用 `AppDb::open` 建出完整
//! schema，用**真实受理路径**写进旧组织的行，再把 v5 的版本登记删掉——此时这个库
//! 就是一个「未迁移过旧行的 v4 库」。走真实受理路径而不是手写 INSERT，是为了让
//! 落库的 JSON 投影（`owner_json` / `plan_json` / `progress_json`）与生产一致，
//! 迁移后的读取路径才被真正跑到。
use super::jobs::SqliteJobRepository;
use super::{AppDb, SCHEMA_VERSION};
use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::dto::job::{JobDefinition, JobFilter};
use datazen_platform_api::id::{
    ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId, Timestamp,
};
use datazen_platform_api::ports::job::JobRepository;
use datazen_runtime::job::{JobClock, SharedClock};
use rusqlite::params;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

/// 迁移三件套与启动期恢复扫描曾经写死的**旧**组织 id（非规范值）。
const LEGACY_ORGANIZATION_ID: &str = "datazen-local";

/// 三条迁移链落库时用的主体（**本波不动**：见 §8 的「主体轴延后」）。
const MIGRATION_PRINCIPAL_ID: &str = "datazen-local-user";

/// 三条迁移链落库时用的客户端实例 id（**本波不动**）。
const MIGRATION_CLIENT_INSTANCE_ID: &str = "datazen-local-client";

fn clock() -> Arc<dyn JobClock> {
    Arc::new(SharedClock::at("2026-01-01T00:00:00Z"))
}

/// **旧**组织 + 迁移链主体的上下文（复刻 2026-10-09 那个版本的行为）。
fn legacy_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(LEGACY_ORGANIZATION_ID),
        PrincipalId::new(MIGRATION_PRINCIPAL_ID),
        None,
        ClientInstanceId::new(MIGRATION_CLIENT_INSTANCE_ID),
        RequestId::new("org-unify-legacy"),
        None,
    )
}

/// **规范**组织的上下文。
///
/// 主体刻意仍取迁移链的字面量主体：本用例只断言**组织维度**被统一了，
/// 主体轴是**单独排期**的行为变更（统一主体会让这批行突然出现在通用任务中心里）。
fn canonical_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(crate::platform::LOCAL_ORGANIZATION_ID),
        PrincipalId::new(MIGRATION_PRINCIPAL_ID),
        None,
        ClientInstanceId::new(MIGRATION_CLIENT_INSTANCE_ID),
        RequestId::new("org-unify-canonical"),
        None,
    )
}

fn apply_definition(job_id: &str, plan_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        // 受理时会**被 ctx 覆盖**（INV-01），这里写什么都一样。
        organization_id: OrganizationId::new(LEGACY_ORGANIZATION_ID),
        kind: "schemaDiffApply".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(MIGRATION_CLIENT_INSTANCE_ID),
            purpose: "migration".into(),
        },
        payload: json!({
            "consumedPlanId": plan_id,
            "planId": plan_id,
            "planDigest": "sha256:abcd1234",
            "selectionRevision": 7,
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        }),
        created_at: Timestamp::new("2025-12-31T23:59:00Z"),
    }
}

/// 要种下的一行。
struct SeedRow<'a> {
    ctx: &'a RequestContext,
    job_id: &'a str,
    plan_id: &'a str,
}

/// 造一个「v4 库 + 若干旧组织的行」。
async fn seed_v4_database(dir: &Path, rows: &[SeedRow<'_>]) {
    let db = AppDb::open(dir).expect("seed database opens");
    let repo = SqliteJobRepository::new(db.clone(), clock(), 30);
    for row in rows {
        repo.accept(
            row.ctx,
            apply_definition(row.job_id, row.plan_id),
            &IdempotencyKey::new(format!("org-unify-idem-{}", row.job_id)),
        )
        .await
        .expect("legacy job is durably admitted");
    }
    // 版本回退：v5 未登记 ⇒ 下次打开时执行 v5 迁移。
    db.with_conn(|conn| {
        conn.execute("DELETE FROM schema_migrations WHERE version >= 5", [])?;
        Ok(())
    })
    .expect("schema version rewound to v4");
    drop(repo);
    drop(db);
}

fn schema_version(db: &AppDb) -> i32 {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?)
    })
    .expect("schema version is readable")
}

fn stored_organization(db: &AppDb, job_id: &str) -> String {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT organization_id FROM jobs WHERE job_id=?1",
            params![job_id],
            |row| row.get(0),
        )?)
    })
    .expect("stored organization is readable")
}

fn stored_principal(db: &AppDb, job_id: &str) -> String {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT owner_principal_id FROM jobs WHERE job_id=?1",
            params![job_id],
            |row| row.get(0),
        )?)
    })
    .expect("stored principal is readable")
}

fn stored_receipt_organization(db: &AppDb, job_id: &str) -> String {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT organization_id FROM job_idempotency_receipts WHERE job_id=?1",
            params![job_id],
            |row| row.get(0),
        )?)
    })
    .expect("stored receipt organization is readable")
}

/// **迁移用例**：旧组织的行在重开 `AppDb` 后并回规范值，且能被**规范 ctx** 列举到。
#[tokio::test]
async fn v5_migration_moves_legacy_organization_rows_to_the_canonical_value() {
    let temp = tempfile::tempdir().expect("temp directory");
    let legacy = legacy_context();
    seed_v4_database(
        temp.path(),
        &[SeedRow {
            ctx: &legacy,
            job_id: "job-legacy-org",
            plan_id: "plan-legacy-org",
        }],
    )
    .await;

    // 重开 ⇒ v5 迁移跑一遍。
    let db = AppDb::open(temp.path()).expect("v4 database migrates to v5");
    assert_eq!(
        schema_version(&db),
        SCHEMA_VERSION,
        "版本环必须推进到 v5，否则迁移没跑"
    );

    // 组织列并回规范值。
    assert_eq!(
        stored_organization(&db, "job-legacy-org"),
        crate::platform::LOCAL_ORGANIZATION_ID,
        "旧组织的行必须并回规范值"
    );

    // 幂等回执的主键**含组织**：不改就成孤儿（永远查不到）。
    assert_eq!(
        stored_receipt_organization(&db, "job-legacy-org"),
        crate::platform::LOCAL_ORGANIZATION_ID,
        "幂等回执的组织必须一并并回规范值（主键含组织）"
    );

    // 规范 ctx 列举得到这一行（列举按 org **且** principal 过滤）。
    let repo = SqliteJobRepository::new(db, clock(), 30);
    let ctx = canonical_context();
    let listed = repo
        .list(
            &ctx,
            JobFilter {
                organization_id: ctx.organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .expect("canonical context lists the migrated row");
    assert_eq!(
        listed.len(),
        1,
        "迁移后的行必须能被规范 ctx 看到：{listed:?}"
    );
    assert_eq!(listed[0].view.job_id, JobId::new("job-legacy-org"));
}

/// **撞唯一约束不导致启动失败**。
///
/// `jobs` 上有 `UNIQUE (organization_id, consumed_plan_id)`：同一 `consumed_plan_id`
/// 在两个组织下都存在时，直接 UPDATE 会撞约束 ⇒ 迁移事务失败 ⇒ 应用起不来。
/// 防御谓词把 UPDATE 限制成「不会撞」的行，撞上的那行留在原地。
#[tokio::test]
async fn a_clashing_consumed_plan_id_does_not_break_startup() {
    let temp = tempfile::tempdir().expect("temp directory");
    let legacy = legacy_context();
    let canonical = canonical_context();
    // 同一个 `consumed_plan_id`，两个不同组织：迁移时旧行改过去就会撞唯一约束。
    seed_v4_database(
        temp.path(),
        &[
            SeedRow {
                ctx: &canonical,
                job_id: "job-canonical-plan",
                plan_id: "plan-clash",
            },
            SeedRow {
                ctx: &legacy,
                job_id: "job-legacy-plan",
                plan_id: "plan-clash",
            },
        ],
    )
    .await;

    // 重开必须**成功**（不 panic、不返回错误）：迁移失败会让应用起不来。
    let db = AppDb::open(temp.path()).expect("迁移撞唯一约束时也不得让启动失败");
    assert_eq!(schema_version(&db), SCHEMA_VERSION);

    // 撞上的那行留在原地：它本来就读不出来（列举按组织过滤），不比现在更糟。
    assert_eq!(
        stored_organization(&db, "job-legacy-plan"),
        LEGACY_ORGANIZATION_ID,
        "撞唯一约束的行必须留在原地，而不是把迁移搞失败"
    );
    // 不撞的那一行照常并回规范值；规范行本身不受影响。
    assert_eq!(
        stored_organization(&db, "job-canonical-plan"),
        crate::platform::LOCAL_ORGANIZATION_ID,
        "规范行不得被迁移改动"
    );

    // 应用仍然可用：规范 ctx 读得到规范行。
    let repo = SqliteJobRepository::new(db, clock(), 30);
    let listed = repo
        .list(
            &canonical,
            JobFilter {
                organization_id: canonical.organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .expect("canonical context still lists after the migration");
    assert_eq!(listed.len(), 1, "实得 {listed:?}");
    assert_eq!(listed[0].view.job_id, JobId::new("job-canonical-plan"));
}

/// **主体维度不受影响**：本波只改组织，`owner_principal_id` 逐字不动。
///
/// 理由：启动期恢复扫描第 2 段的主体必须与三条链落库的 `owner_principal_id` 一致；
/// 认领栅栏（`jobs/owner.rs` 的 `claim_client_instance_id=?8`）也依赖它。
/// 动了主体会打断恢复扫描与认领校验。
#[tokio::test]
async fn v5_migration_leaves_the_principal_dimension_untouched() {
    let temp = tempfile::tempdir().expect("temp directory");
    let legacy = legacy_context();
    seed_v4_database(
        temp.path(),
        &[SeedRow {
            ctx: &legacy,
            job_id: "job-principal-untouched",
            plan_id: "plan-principal-untouched",
        }],
    )
    .await;

    let db = AppDb::open(temp.path()).expect("v4 database migrates to v5");
    assert_eq!(
        stored_principal(&db, "job-principal-untouched"),
        MIGRATION_PRINCIPAL_ID,
        "迁移只动组织维度：主体必须逐字不变"
    );
    // 主体没动 ⇒ 迁移后的行仍能被**同一主体**的 ctx 读到。
    let repo = SqliteJobRepository::new(db, clock(), 30);
    let ctx = canonical_context();
    let listed = repo
        .list(
            &ctx,
            JobFilter {
                organization_id: ctx.organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .expect("migrated row is listed under the same principal");
    assert_eq!(listed.len(), 1, "实得 {listed:?}");
}
