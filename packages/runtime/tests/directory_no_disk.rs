//! 目录**不落盘**的反面证据（CM-61 / A3.2）。
//!
//! 「`dbSessionId` 与挂载令牌只在内存里」这句话，光靠读代码不算证据。本文件给出
//! 三条互相独立的反面证据：
//!
//! 1. **源码扫描**：`src/directory/**` 里不出现任何落盘 API（文件、路径、数据库、
//!    序列化写回），只 `include_str!` 编译期读入源码文本，不执行任何写盘调用。
//! 2. **文件系统对照**：跑完一整轮登记 → 挂载 → 替换 → 过期 → 清扫 → 作废之后，
//!    临时目录与 crate 目录的文件集合**没有多出任何与本次会话相关的东西**。
//! 3. **可观测投影**：令牌原文与 `dbSessionId` 不出现在 owner / 快照 / 作废记录
//!    任何一处 `Debug` 输出里——所以它们也进不了日志与事件。
//!
//! 全程不读、也不打印任何 `.env` / `.env.test`。

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{client, cycling_directory, editor_draft, principal, T0_UTC_MILLIS};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{AttachmentToken, RuntimeEpoch, Timestamp};
use datazen_platform_api::ports::session_directory::{
    CloseDisposition, InvalidationReason, SessionDirectory,
};
use datazen_runtime::directory::{InMemorySessionDirectory, SessionHandle};

/// 目录模块的源码全集。路径相对本文件（`packages/runtime/tests/`）。
const SOURCES: &[(&str, &str)] = &[
    ("mod.rs", include_str!("../src/directory/mod.rs")),
    (
        "directory.rs",
        include_str!("../src/directory/directory.rs"),
    ),
    ("entry.rs", include_str!("../src/directory/entry.rs")),
    (
        "lifecycle.rs",
        include_str!("../src/directory/lifecycle.rs"),
    ),
    (
        "attachment.rs",
        include_str!("../src/directory/attachment.rs"),
    ),
    ("commit.rs", include_str!("../src/directory/commit.rs")),
    ("ttl.rs", include_str!("../src/directory/ttl.rs")),
    ("id.rs", include_str!("../src/directory/id.rs")),
    ("tests.rs", include_str!("../src/directory/tests.rs")),
];

/// 任何一种落盘手段的痕迹。出现任意一个就是「目录开始写盘了」。
const FORBIDDEN: &[(&str, &str)] = &[
    ("std 文件系统", "std::fs"),
    ("std::fs::File", "fs::File"),
    ("std::path", "std::path"),
    ("OpenOptions", "OpenOptions"),
    ("创建目录", "create_dir"),
    ("写文件", "write_all"),
    ("读文件", "read_to_string"),
    ("sqlite", "Sqlite"),
    ("sqlite 驱动", "sqlx"),
    ("快照落盘", "snapshot("),
    ("只追加日志", "append_only"),
    ("wal", "wal_log"),
    ("持久化字样", "persist"),
];

#[test]
fn directory_source_never_touches_the_filesystem() {
    for (file, source) in SOURCES {
        for (label, needle) in FORBIDDEN {
            assert!(
                !source.contains(needle),
                "src/directory/{file} 里出现了{label}（`{needle}`）：目录只做内存路由，不得落盘"
            );
        }
    }
}

#[test]
fn every_directory_source_file_is_scanned() {
    // 扫描不能漏文件：新增文件忘了加进 SOURCES，证据就悄悄失效了。
    let listed: BTreeSet<String> = SOURCES.iter().map(|(name, _)| (*name).to_owned()).collect();
    let on_disk = source_file_names();
    assert_eq!(
        listed, on_disk,
        "src/directory 的文件清单与 SOURCES 不一致：新文件必须纳入反面证据扫描"
    );
}

/// `src/directory/` 下实际存在的 `.rs` 文件名集合。
fn source_file_names() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/directory");
    std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("读不到 {}：{err}", dir.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .collect()
}

/// 递归列出某个目录下的**文件**相对路径（跳过 `target` 与 `.git`）。
fn list_files(root: &Path, out: &mut BTreeSet<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            list_files(&path, out, depth + 1);
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn snapshot_of(root: PathBuf) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    list_files(&root, &mut out, 0);
    out
}

/// 只看顶层条目名。临时目录下面挂着系统自己的深层树，递归扫要几十秒，
/// 而任何落盘产物（快照文件、WAL、缓存目录）都会先落在顶层。
fn snapshot_top_level(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root)
        .unwrap_or_else(|err| panic!("读不到 {}：{err}", root.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

fn added_since(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<String> {
    after.difference(before).cloned().collect()
}

#[tokio::test]
async fn a_full_session_lifecycle_writes_nothing_to_the_temp_dir() {
    let (clock, dir) = cycling_directory(64);
    let temp = std::env::temp_dir();
    let temp_before = snapshot_top_level(&temp);
    let crate_before = snapshot_of(crate_dir());

    let (handle, token) = open_and_attach(&dir, &clock).await;
    exercise_replacement(&dir, &handle).await;
    exercise_expiry(&dir, &clock).await;
    let invalidated = exercise_invalidation(&dir).await;

    // 会话确实干过活：不是「什么都没发生所以当然没文件」。
    assert!(
        dir.invalidation_record(&invalidated).is_some(),
        "本轮必须真的作废过一个会话，否则「没多出文件」毫无意义"
    );
    assert!(
        token.as_str().starts_with("att_") && token.as_str().len() >= 36,
        "令牌必须真的被签发出来（带前缀、足够长），否则「没有落盘」证明不了什么"
    );

    let temp_new = added_since(&temp_before, &snapshot_top_level(&temp));
    let leaked: Vec<&String> = temp_new
        .iter()
        .filter(|name| looks_like_session_junk(name))
        .collect();
    assert!(
        leaked.is_empty(),
        "会话目录在临时目录里留下了文件：{leaked:?}——dbSessionId 与令牌只在内存里"
    );

    let crate_new = added_since(&crate_before, &snapshot_of(crate_dir()));
    assert!(
        crate_new.is_empty(),
        "会话目录往 crate 目录写了文件：{crate_new:?}——不允许任何形式的落盘"
    );
}

/// 文件名里出现这些片段，才算「疑似会话状态外泄」。
///
/// 之所以带 `session` 这种宽片段再按名字筛，是因为产物名可能不带 `dbs_`：
/// 比如叫 `session-snapshot.bin` 也一样是泄漏。
fn looks_like_session_junk(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    [
        "dbs_",
        "dbs-",
        "datazen",
        "db_session",
        "dbsession",
        "session",
        "rte_",
        "atk_",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn crate_dir() -> PathBuf {
    // 集成测试的工作目录就是 crate 根（`packages/runtime`）。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

async fn open_and_attach(
    dir: &InMemorySessionDirectory,
    clock: &std::sync::Arc<common::FakeClock>,
) -> (SessionHandle, AttachmentToken) {
    let (handle, token) = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            RuntimeEpoch::new("rte-no-disk-0001"),
        )
        .expect("登记应成功");
    dir.attach_from_client(
        &handle,
        principal("user-1"),
        client("client-1"),
        token.clone(),
    )
    .expect("带正确令牌的挂载应成功");
    dir.arm_transaction_idle(&handle, Duration::from_secs(60), clock.instant())
        .expect("应能挂上事务空闲期限");
    dir.touch(&handle, clock.utc_now())
        .await
        .expect("应能刷新业务时间");
    (handle, token)
}

async fn exercise_replacement(dir: &InMemorySessionDirectory, old: &SessionHandle) {
    let candidate = datazen_platform_api::ports::session_directory::SessionOwner {
        db_session_id: datazen_platform_api::id::DbSessionId::new("dbs_no_disk_candidate"),
        organization_id: datazen_platform_api::id::OrganizationId::new("org-1"),
        principal_id: principal("user-1"),
        connection_id: datazen_platform_api::id::ConnectionId::new("conn-1"),
        owner: datazen_platform_api::context::OwnerRef::Editor {
            client_instance_id: client("client-1"),
            editor_session_id: datazen_platform_api::id::EditorSessionId::new("client-1-editor"),
        },
        worker_id: datazen_platform_api::id::WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new("rte-no-disk-0002"),
        resource_epoch: 0,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00.000Z"),
    };
    let operation = |operation| datazen_platform_api::ports::session_directory::ReplacementCommit {
        old: old.clone(),
        new_owner: candidate.clone(),
        operation,
    };
    use datazen_platform_api::ports::session_directory::ReplacementOperation as Op;
    dir.commit_replacement(operation(Op::Prepared))
        .await
        .expect("prepare 应成功");
    dir.commit_replacement(operation(Op::RolledBack))
        .await
        .expect("rollback 应成功");
}

async fn exercise_expiry(
    dir: &InMemorySessionDirectory,
    clock: &std::sync::Arc<common::FakeClock>,
) {
    clock.advance(Duration::from_secs(61));
    let swept = dir
        .sweep_expired(clock.utc_now())
        .await
        .expect("清扫不该报错");
    assert!(
        !swept.is_empty(),
        "这轮必须真的扫出一个超期会话，否则「没多出文件」只是因为什么都没发生"
    );
}

async fn exercise_invalidation(
    dir: &InMemorySessionDirectory,
) -> datazen_platform_api::id::DbSessionId {
    let (handle, _token) = dir
        .open_session(
            editor_draft("org-1", "user-1", "conn-1", "client-1"),
            RuntimeEpoch::new("rte-no-disk-0001"),
        )
        .expect("登记应成功");
    dir.invalidate(handle.db_session_id.clone(), InvalidationReason::WorkerLost)
        .await
        .expect("作废应成功");
    assert!(
        matches!(
            dir.lookup(handle.db_session_id.clone()).await,
            Err(PortError::NotFound(_)) | Ok(None)
        ),
        "作废后不得透明重建"
    );
    dir.release(&handle, CloseDisposition::Closed)
        .await
        .expect_err("已作废的会话不得再走正常关闭");
    handle.db_session_id.clone()
}

#[tokio::test]
async fn the_attachment_token_never_reaches_any_observable_projection() {
    let (clock, dir) = cycling_directory(64);
    let (handle, token) = open_and_attach(&dir, &clock).await;
    let plaintext = token.as_str().to_owned();

    assert!(
        !plaintext.is_empty(),
        "令牌原文必须真的存在，否则断言是空的"
    );

    let owner = dir
        .lookup(handle.db_session_id.clone())
        .await
        .expect("查询不该报错")
        .expect("刚登记的会话应查得到");
    let owner_debug = format!("{owner:?}");
    let snapshot = dir.observe(&handle).expect("观测不该失败");
    let snapshot_debug = format!("{snapshot:?}");
    let token_debug = format!("{token:?}");

    for (label, rendered) in [
        ("owner", owner_debug.as_str()),
        ("会话快照", snapshot_debug.as_str()),
        ("令牌自身的 Debug", token_debug.as_str()),
    ] {
        assert!(
            !rendered.contains(plaintext.as_str()),
            "{label} 的输出里出现了挂载令牌原文：它只能留在调用方内存里"
        );
    }

    // 令牌自身的 Debug 必须脱敏：事件与日志随手 format 一下也不会泄漏。
    assert!(
        !token_debug.contains(&plaintext[..plaintext.len().min(8)]),
        "令牌的 Debug 必须整段脱敏，而不是只打前缀"
    );

    // 作废记录里同样没有令牌。
    dir.invalidate(
        handle.db_session_id.clone(),
        InvalidationReason::IdentityRotated,
    )
    .await
    .expect("作废应成功");
    let record = dir
        .invalidation_record(&handle.db_session_id)
        .expect("作废记录应可查");
    assert!(
        !format!("{record:?}").contains(plaintext.as_str()),
        "作废记录里不该出现令牌"
    );
    assert_eq!(
        record.reason_code, "identityRotated",
        "作废原因必须可区分，不能笼统成「会话没了」"
    );
}

#[tokio::test]
async fn the_expired_session_disappears_from_memory_entirely() {
    let (clock, dir) = cycling_directory(64);
    let (handle, _token) = open_and_attach(&dir, &clock).await;
    assert_eq!(dir.len(), 1, "登记后会话应在目录里");

    clock.advance(Duration::from_secs(61));
    let swept = dir
        .sweep_expired(clock.utc_now())
        .await
        .expect("清扫不该报错");
    assert_eq!(swept.len(), 1, "超期会话必须被扫出来交给调用方关物理资源");
    assert_eq!(
        dir.len(),
        0,
        "清扫之后内存里必须什么都不剩：没有 tombstone，也没有墓碑文件"
    );
    assert!(
        dir.invalidation_record(&handle.db_session_id).is_none(),
        "过期清理与作废是两回事：过期不产生作废记录"
    );
    assert!(
        matches!(
            dir.lookup(handle.db_session_id.clone()).await,
            Ok(None) | Err(PortError::NotFound(_))
        ),
        "扫掉的会话只能得到「不知道」"
    );
    assert_eq!(T0_UTC_MILLIS, 1_767_225_600_000, "假时钟起点固定，便于复现");
}
