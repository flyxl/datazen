//! 目录**不落盘**的反面证据（CM-61 / A3.2）。
//!
//! 「`dbSessionId` 与挂载令牌只在内存里」这句话，光靠读代码不算证据。本文件给出
//! 三条互相独立的反面证据：
//!
//! 1. **源码扫描**：`src/directory/**` 里不出现任何落盘 API（文件、路径、数据库、
//!    序列化写回），只 `include_str!` 编译期读入源码文本，不执行任何写盘调用。
//! 2. **文件系统对照**：跑完一整轮登记 → 挂载 → 替换 → 过期 → 清扫 → 作废之后，
//!    一个**私有的、空的**临时目录与 crate 目录**没有多出任何条目**。判据是
//!    「跑完之后这里仍然是空的」，**与文件名无关**——任何落盘产物都不需要自报家门。
//! 3. **可观测投影**：令牌原文与 `dbSessionId` 不出现在 owner / 快照 / 作废记录
//!    任何一处 `Debug` 输出里——所以它们也进不了日志与事件。
//!
//! 门禁本身也要能证明自己有效，所以另有一条用例**故意栽一个泄漏**并断言探测器会响。
//!
//! 全程不读、也不打印任何 `.env` / `.env.test`。

mod common;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
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

/// 只看顶层条目名（不 stat、不递归）。
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

/// 落盘门禁的观测点：一个**空的、私有的**临时目录。
///
/// 不用系统临时目录本身，有两个硬理由：
///
/// - `$TMPDIR` 是**全机共享**的。这台机器上它有十万量级的顶层条目、十七万个目录；
///   递归扫一遍要 30 秒，而 `rustc` 还在随时往里丢临时文件——同一个断言既慢，
///   又会被同机其他编译误判成失败。
/// - 私有空目录把基线钉死成「**什么都没有**」，于是门禁退化成「跑完之后这里仍然是
///   空的」，**与文件名无关**。
///
/// 盯的仍然是同一个 API：生产代码一旦调用 `std::env::temp_dir()`，拿到的就是这个
/// 私有目录，落盘照样现形。名字无关还顺带堵住了「别名导入绕过源码扫描」这条路
/// （`use std::fs as f;` 躲得过 `FORBIDDEN` 字面量，躲不过目录里多出条目）。
///
/// `$TMPDIR` 是进程级全局量，因此所有用它的用例都持同一把锁排队；`Drop` 保证
/// panic 路径也会把环境变量与临时目录还原掉，不留残留。
static TEMP_ROOT_LOCK: Mutex<()> = Mutex::new(());

struct PrivateTempRoot {
    path: PathBuf,
    previous: Option<OsString>,
    _serial: MutexGuard<'static, ()>,
}

impl PrivateTempRoot {
    fn new(tag: &str) -> Self {
        // 锁中毒也要能继续跑：这里保护的是「临时目录归属」，不是不变量。
        let serial = TEMP_ROOT_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("TMPDIR");
        let path = std::env::temp_dir().join(format!("dz-no-disk-{tag}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("私有临时目录应能建立");
        std::env::set_var("TMPDIR", &path);
        assert!(
            snapshot_top_level(&path).is_empty(),
            "私有临时目录的基线必须是空的，否则「结束后仍为空」证明不了任何事"
        );
        Self {
            path,
            previous,
            _serial: serial,
        }
    }
}

impl Drop for PrivateTempRoot {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("TMPDIR", value),
            None => std::env::remove_var("TMPDIR"),
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 私有临时目录里出现的所有产物路径。
///
/// **顶层多出任何条目就是泄漏，不看名字**——这一行是本门禁的全部要害。
/// 这里曾经有一张「名字里带 `dbs_` / `session` / `datazen` 才算可疑」的名单，
/// 于是 `dzcache/state.bin` 这种一个可疑词都不含的产物可以大摇大摆走过去。
/// 递归展开只是为了让断言消息能指出到底写了哪个文件。
fn leaked_paths(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for name in snapshot_top_level(root) {
        let path = root.join(&name);
        out.push(name.clone());
        if path.is_dir() {
            let mut children = BTreeSet::new();
            list_files(&path, &mut children, 1);
            out.extend(children.into_iter().map(|child| format!("{name}/{child}")));
        }
    }
    out
}

#[tokio::test]
async fn a_full_session_lifecycle_writes_nothing_to_the_temp_dir() {
    let temp = PrivateTempRoot::new("lifecycle");
    let crate_before = snapshot_of(crate_dir());
    let (clock, dir) = cycling_directory(64);

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

    let leaked = leaked_paths(&temp.path);
    assert!(
        leaked.is_empty(),
        "会话目录在临时目录里留下了东西：{leaked:?}——dbSessionId 与令牌只在内存里。\
         这里不按文件名筛选，多出一个顶层条目就是泄漏"
    );

    let crate_new = added_since(&crate_before, &snapshot_of(crate_dir()));
    assert!(
        crate_new.is_empty(),
        "会话目录往 crate 目录写了文件：{crate_new:?}——不允许任何形式的落盘"
    );
}

/// 门禁自身的灵敏度自检。
///
/// 在生产代码干净的时候，上面那条用例**永远是绿的**——一个什么都抓不住的门禁和一个
/// 抓得住的门禁，在这条用例上表现一模一样。所以这里先**故意栽一个泄漏**，证明探测器
/// 确实会响，再由 `Drop` 清理干净。
///
/// 栽的形态刻意毫无特征：`plain-name/state.bin` 不含任何可疑词，
/// 这正是旧名单漏掉的那一类产物。
#[test]
fn the_leak_detector_reports_a_planted_entry_however_it_is_named() {
    let temp = PrivateTempRoot::new("detector");
    assert!(
        leaked_paths(&temp.path).is_empty(),
        "基线必须为空，否则这个自检什么也证明不了"
    );

    std::fs::create_dir_all(temp.path.join("plain-name")).expect("应能造出目录");
    std::fs::write(temp.path.join("plain-name/state.bin"), b"leak").expect("应能写出文件");

    assert_eq!(
        leaked_paths(&temp.path),
        vec!["plain-name".to_owned(), "plain-name/state.bin".to_owned()],
        "探测器必须把不带任何可疑词的顶层条目、以及它下面的文件一并点出来"
    );
}

fn crate_dir() -> PathBuf {
    // 集成测试的工作目录就是 crate 根（`packages/runtime`）。
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

async fn open_and_attach(
    dir: &InMemorySessionDirectory,
    clock: &std::sync::Arc<common::TestClock>,
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
    clock: &std::sync::Arc<common::TestClock>,
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
