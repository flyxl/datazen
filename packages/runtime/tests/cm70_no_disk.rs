//! 令牌与回执**不落盘**的反面证据（CM-70 / A8）。
//!
//! `connection-management.md:1296-1297` 的断言之一是「运行时 receipt/token 不落盘」。
//! 这句话光靠读代码不算证据，所以本文件给出三条互相独立的反面证据：
//!
//! 1. **源码扫描**：`src/gateway/**` 里不出现任何落盘手段（文件系统、路径、SQLite、
//!    内嵌 KV 引擎、只追加日志）。只 `include_str!` 在编译期读入源码文本，运行期不调用
//!    任何写盘 API。另有 `every_gateway_source_file_is_scanned`：新增文件忘了登记进
//!    `SOURCES`，证据会**悄悄失效**——这一条专门防这个。
//! 2. **文件系统对照**：先把 `TMPDIR` 指向一个**私有的、启动时为空**的目录，跑完整整一轮
//!    CM-70（签发 → 受理 → 下发 → 重放 → 过期 → 保留期清扫 → 过期后重放 → 结局未知围栏），
//!    之后断言该目录**仍然是空的**，并且 crate 目录的条目集合**一个都没多**。
//!    判据是「跑完之后这里仍然是空的」，**与文件名无关**——落盘产物不需要自报家门。
//! 3. **可观测投影**：签名密钥与令牌原文不出现在令牌层任何一处 `Debug` 输出里，
//!    所以它们也进不了日志与事件。
//!
//! 门禁本身也要能证明自己有效，所以另有一条用例**故意栽一个泄漏**并断言探测器会响。
//!
//! 第 3 条**不**声称「任何 `Debug` 都不含令牌」：CM-54 那条冻结的账本作用域
//! （`IdempotencyScope`）按设计就带着 `idempotencyKey`，也就是令牌本身，它在内存里。
//! 「只在内存」这句话由第 2 条兜底，不靠 `Debug` 形状来断言——那是另一种性质的主张。
//!
//! 全程不读、也不打印任何 `.env` / `.env.test`。
//!
//! ## panic 文本里能出现什么
//!
//! 本文件每条断言都在检查「凭据别漏进输出面」，所以它自己的 panic 文本**也是**输出面：
//! 一次失败的断言会把 panic 文本写进 CI 日志，等于把凭据原样交给任何有日志读取权的人。
//! 判定只有一条标准：**插进去的值是否可能承载凭据**。
//!
//! 允许出现的四类：
//! 1. 编译期常量（字面量，或只由 `SOURCES` / `FORBIDDEN` 这类登记表派生出来的值）；
//! 2. 逐字段在**构造点**核过、确认不含凭据的类型——本文件只用到 `GatewayError`，它的字段
//!    全是语义指纹（`gateway/mod.rs` 里的 `incoming.as_str()`）、`ExecutionId`、`&'static str`
//!    与网关自建消息，没有一处是签名后的令牌原文；
//! 3. 计数、下标、文件名这类定长或非敏感标量；
//! 4. 本文件自造的合成字面量（如 `DRIVER_TEXT`）。
//!
//! 被排除的，逐处理由：
//! - `token` / `rendered` / `segment` / `parts`：前两个一个是被查的凭据本身，一个是**正被
//!   怀疑装了凭据**的那段渲染文本；插进 panic 文本就是把一次失败变成一次泄漏。`credential_segments`
//!   的段数断言尤其致命：它在**任何泄漏检查之前**触发，所以该助手任何一次失败都会把四段真凭据
//!   打进日志。
//! - 目录条目清单（`temp.entries()` / `added`）：被插进去的值来自**被怀疑的那份状态本身**，
//!   没有任何上游断言为它背书。真落盘了靠「条目数不为零」发现，而不是靠把名字打出来。
//! - `incoming`：它同样是那条 JSON 投影里的被查值，只是长度另有断言兜着；长度断言挡不住
//!   「长到不像指纹的凭据」，所以改成回显长度。
//!
//! `{error:?}` / `{err:?}` / `{other:?}` / `{message}` / `{unreadable:?}` / `{fenced:?}` /
//! `{conflict:?}` / `{forged:?}` 一律**保留**：它们是上面第 2 类（`GatewayError`），或本仓测试夹具
//! 自造的驱动文本——`tests/` 下每个 `RuntimeError` 载荷都是测试自有的字面量，不来自真实驱动。
//! `{want}`（`refused`）同理，每个调用点传的都是字面量标签。
//!
//! 跨目录的同一轮裁定与机器闸门见 `tests/cm70_panic_redaction_guard.rs`。

// 本二进制只用到夹具的一部分（`tests/gateway_fixtures` 同时服务 `gateway_contract`
// 与 CM-70 主二进制），按 `registry_*` 那组测试二进制同样的做法把整份夹具静音，
// 免得凭空多出几十条「夹具没被用到」的编译告警——那种告警只会训练人忽略告警。
#![allow(dead_code)]

mod gateway_fixtures;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use datazen_runtime::connection::{ExecutionId, RuntimeError};
use datazen_runtime::gateway::{
    GatewayAction, GatewayError, MonotonicClock, TokenKeyring, RETENTION_AFTER_EXPIRY_NANOS,
};
use gateway_fixtures as fx;

/// 网关层的源码全集。路径相对本文件（`packages/runtime/tests/`）。
const SOURCES: &[(&str, &str)] = &[
    ("mod.rs", include_str!("../src/gateway/mod.rs")),
    ("request.rs", include_str!("../src/gateway/request.rs")),
    (
        "owner_binding.rs",
        include_str!("../src/gateway/owner_binding.rs"),
    ),
    (
        "request_cm06_negatives.rs",
        include_str!("../src/gateway/request_cm06_negatives.rs"),
    ),
    (
        "idempotency.rs",
        include_str!("../src/gateway/idempotency.rs"),
    ),
    ("token.rs", include_str!("../src/gateway/token.rs")),
    ("retention.rs", include_str!("../src/gateway/retention.rs")),
    ("events.rs", include_str!("../src/gateway/events.rs")),
    ("cancel.rs", include_str!("../src/gateway/cancel.rs")),
    (
        "provenance.rs",
        include_str!("../src/gateway/provenance.rs"),
    ),
    ("timing.rs", include_str!("../src/gateway/timing.rs")),
    (
        "facade_support.rs",
        include_str!("../src/gateway/facade_support.rs"),
    ),
    (
        "testing_support.rs",
        include_str!("../src/gateway/testing_support.rs"),
    ),
    (
        "token_tests.rs",
        include_str!("../src/gateway/token_tests.rs"),
    ),
    (
        "retention_tests.rs",
        include_str!("../src/gateway/retention_tests.rs"),
    ),
    (
        "cancel_event_tests.rs",
        include_str!("../src/gateway/cancel_event_tests.rs"),
    ),
    (
        "event_store_tests.rs",
        include_str!("../src/gateway/event_store_tests.rs"),
    ),
    (
        "facade_tests.rs",
        include_str!("../src/gateway/facade_tests.rs"),
    ),
];

/// 任何一种落盘手段的痕迹。出现任意一个就是「运行时令牌开始写盘了」。
///
/// 这里挑的是**能力**（能打开文件、能开数据库、能写 KV），不是**词**。所以刻意不放
/// 「持久化」这类字样：网关里有一族 `to_persistable_json`（错误投影成 JSON），
/// 按词过滤要么误报要么被迫开白名单，而白名单正是「把判据调成永远通过」的老路。
const FORBIDDEN: &[(&str, &str)] = &[
    ("std 文件系统", "std::fs"),
    ("std::fs::File", "fs::File"),
    ("std::path", "std::path"),
    ("构造路径", "Path::new"),
    ("打开文件", "File::open"),
    ("OpenOptions", "OpenOptions"),
    ("创建目录", "create_dir"),
    ("写字节", "write_all"),
    ("读文本", "read_to_string"),
    ("流式写入", "to_writer"),
    ("强制落盘", "fsync"),
    ("sqlite", "sqlite"),
    ("sqlite 驱动", "sqlx"),
    ("内嵌 KV", "sled"),
    ("内嵌 KV", "redb"),
    ("只追加日志", "append_only"),
    ("wal", "wal_log"),
    ("临时目录", "temp_dir"),
];

/// 探测器本体：给定一段源码文本，返回命中的第一条落盘手段（手段名，针）。
///
/// 单测的 `the_detector_catches_a_planted_leak` 靠它证明这套判据不是摆设。
fn detect(source: &str) -> Option<(&'static str, &'static str)> {
    FORBIDDEN
        .iter()
        .find(|(_, needle)| source.contains(needle))
        .map(|(name, needle)| (*name, *needle))
}

#[test]
fn gateway_source_never_touches_the_filesystem() {
    for (file, source) in SOURCES {
        for (label, needle) in FORBIDDEN {
            assert!(
                !source.contains(needle),
                "src/gateway/{file} 里出现了{label}（`{needle}`）：令牌与回执只在内存里，不得落盘"
            );
        }
    }
}

#[test]
fn every_gateway_source_file_is_scanned() {
    // 扫描不能漏文件：新增文件忘了登记进 SOURCES，上面那条证据就悄悄失效了。
    let listed: BTreeSet<String> = SOURCES.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(
        listed,
        source_file_names(),
        "src/gateway 的文件清单与 SOURCES 不一致：新文件必须纳入反面证据扫描"
    );
}

/// `src/gateway/` 下实际存在的 `.rs` 文件名集合。
fn source_file_names() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gateway");
    std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("读不到 {}：{err}", dir.display()))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .collect()
}

/// 目录下的条目名（文件与子目录都算，递归跳过 `target` 与 `.git`）。
fn list_entries(root: &Path, out: &mut BTreeSet<String>, depth: usize) {
    if depth > 3 {
        return;
    }
    let entries = std::fs::read_dir(root)
        .unwrap_or_else(|err| panic!("读不到 {}：{err}", root.display()))
        .flatten();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if depth == 0 && (name == "target" || name == ".git") {
            continue;
        }
        out.insert(name.clone());
        if entry.path().is_dir() {
            list_entries(&entry.path(), out, depth + 1);
        }
    }
}

/// `TMPDIR` 是**进程级全局**量：本二进制内的两个用例并行跑，就会各自把对方的私有根
/// 当成自己的——后建的那个还会把自己的目录建在别人的根**里面**，于是两条断言一起炸在
/// 「起点必须是空的」（实测 HEAD 连跑 25 次只有 7 绿 18 红；基线 13 条里只用到两个根，
/// 竞态窗口小得多）。
///
/// 因此这里给整份私有根加一把**进程内**串行锁，锁在 [`PrivateTempRoot::new`] 内部取得
/// ——「建了根却没持锁」因此在**结构上**不可能发生，用例里没有旁路可走（`Drop` 顺带把锁
/// 还回去，panic 路径也不例外）。与 `tests/directory_no_disk.rs` 的 `TEMP_ROOT_LOCK` 是
/// 同一套写法。
static TEMP_ROOT_LOCK: Mutex<()> = Mutex::new(());

/// 私有空临时根：`TMPDIR` 在本测试期间指向它，之后它必须仍然是空的。
struct PrivateTempRoot {
    path: PathBuf,
    saved: Option<OsString>,
    /// 持锁即独占这个私有根；字段声明在最后，保证它活得比 `path` / `saved` 久。
    _serial: MutexGuard<'static, ()>,
}

impl PrivateTempRoot {
    fn new(label: &str) -> Self {
        // 锁中毒也要能继续跑：它保护的是「临时根归属」，不是任何不变量。
        let serial = TEMP_ROOT_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = std::env::temp_dir().join(format!("dz-cm70-nodisk-{label}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("建不出私有临时根 {}：{err}", path.display()));
        let saved = std::env::var_os("TMPDIR");
        std::env::set_var("TMPDIR", &path);
        Self {
            path,
            saved,
            _serial: serial,
        }
    }

    fn entries(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        list_entries(&self.path, &mut out, 0);
        out
    }
}

impl Drop for PrivateTempRoot {
    fn drop(&mut self) {
        match self.saved.take() {
            Some(value) => std::env::set_var("TMPDIR", value),
            None => std::env::remove_var("TMPDIR"),
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 跑完整整一轮 CM-70，返回这一轮用过的令牌原文（供投影那条证据检查）。
async fn full_cm70_cycle(h: &fx::TokenHarness) -> String {
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let request = fx::request_with_key(fx::REVISION, &token);

    // 写入已接受且真的下发（响应丢失之后要靠重放拿回同一张回执）。
    let first = fx::accept(&h.harness, request.clone()).await;
    h.harness
        .gateway
        .dispatch(&fx::principal(), &first)
        .await
        .unwrap_or_else(|err| panic!("首次派发应成功：{err:?}"));
    // 有效期内重发：同一张回执，且**不再下发**——重发不是重跑。
    let replay = fx::accept(&h.harness, request.clone()).await;
    assert_eq!(first, replay, "有效期内重发必须是同一张回执");
    assert_eq!(h.harness.port.execute_calls(), 1, "重发不得二次执行");

    // 过期后重发：不再执行。
    h.harness.clock.advance(fx::TOKEN_EXPIRES_AT_NANOS + 1);
    assert!(
        h.harness
            .gateway
            .accept(&fx::principal(), request.clone())
            .await
            .is_err(),
        "过期令牌必须被拒"
    );

    // 超过记录保留期：清扫删掉账本记录；再重放同样不执行。
    let now = fx::TOKEN_EXPIRES_AT_NANOS + RETENTION_AFTER_EXPIRY_NANOS;
    let sweep = h.harness.gateway.sweep_idempotency_retention(now);
    assert_eq!(sweep.ledger_deleted, 1, "清扫该删掉那条账本记录");
    assert_eq!(
        h.store.delete_calls(),
        1,
        "删除只应发生一次，且落在账本替身上"
    );
    h.harness.clock.advance(now);
    assert!(
        h.harness
            .gateway
            .accept(&fx::principal(), request)
            .await
            .is_err(),
        "记录删除后重放必须被拒"
    );
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "整轮下来这条写入只执行过一次"
    );
    token
}

/// 断言 2：跑完整轮 CM-70 之后，私有临时根与 crate 目录都没有多出任何东西。
#[tokio::test]
async fn a_full_cm70_cycle_leaves_nothing_on_disk() {
    let temp = PrivateTempRoot::new("cycle");
    let crate_entries_before = {
        let mut out = BTreeSet::new();
        list_entries(Path::new(env!("CARGO_MANIFEST_DIR")), &mut out, 0);
        out
    };
    assert!(temp.entries().is_empty(), "起点必须是空的，否则对照无意义");

    let h = fx::token_harness();
    full_cm70_cycle(&h).await;

    assert!(
        temp.entries().is_empty(),
        "令牌/回执落盘了：临时根多出了非预期条目"
    );
    let mut crate_entries_after = BTreeSet::new();
    list_entries(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &mut crate_entries_after,
        0,
    );
    let added: Vec<&String> = crate_entries_after
        .difference(&crate_entries_before)
        .collect();
    assert!(
        added.is_empty(),
        "crate 目录多出条目：运行时令牌不许留在工作区里"
    );
}

/// 断言 2 的补充：即使跑两轮、并让围栏与清扫都动过，仍然不留痕。
#[tokio::test]
async fn two_cycles_and_a_retention_sweep_still_leave_nothing_on_disk() {
    let temp = PrivateTempRoot::new("twice");
    let h = fx::token_harness();
    full_cm70_cycle(&h).await;

    // 第二轮：这一次让账本读不出来，把「结局未知」的围栏也走一遍。
    h.store.fail_reads();
    // 上一轮已经把时钟推过保留期；就按**此刻**的时刻再签一张——这样它一定还没到期，
    // 后面被拒的只可能是因为围栏，而不是又混进了「令牌过期」这条噪声。
    let token = fx::issue_token(&h.tokens, h.harness.clock.now_nanos());
    let request = fx::request_with_key(fx::REVISION, &token);
    let unreadable = h
        .harness
        .gateway
        .accept(&fx::principal(), request.clone())
        .await
        .expect_err("账本读不出来必须要求核验");
    assert!(
        matches!(
            unreadable,
            datazen_runtime::gateway::GatewayError::IdempotencyVerificationRequired { .. }
        ),
        "要的是「结局未知、请核验」，实际是 {unreadable:?}"
    );
    h.store.recover_reads();
    let fenced = h
        .harness
        .gateway
        .accept(&fx::principal(), request)
        .await
        .expect_err("换新键自动重试未知写入必须仍被拒");
    assert!(
        matches!(
            fenced,
            datazen_runtime::gateway::GatewayError::IdempotencyVerificationRequired { .. }
        ),
        "要的还是「结局未知、请核验」，实际是 {fenced:?}"
    );
    assert_eq!(h.harness.gateway.unverified_outcome_count().await, 1);

    assert!(temp.entries().is_empty(), "两轮之后临时根多出了非预期条目");
}

/// 断言 3：签名密钥与令牌原文不进任何 `Debug` 输出。
#[test]
fn the_signing_secret_and_the_token_stay_out_of_debug_output() {
    let keyring = TokenKeyring::single(fx::SIGNING_KEY.to_vec());
    // 派生 Debug 会把密钥打成十进制字节串 `[99, 109, 55, …]`；这条断言连那种形状一起挡。
    let decimal = fx::SIGNING_KEY
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let rendered = format!("{keyring:?}");
    assert!(
        !rendered.contains(std::str::from_utf8(fx::SIGNING_KEY).expect("夹具密钥是 ASCII")),
        "TokenKeyring 的 Debug 泄出了密钥原文"
    );
    assert!(
        !rendered.contains(&decimal),
        "TokenKeyring 的 Debug 泄出了密钥字节"
    );
    assert!(
        rendered.contains("TokenKeyring") && rendered.contains("current"),
        "脱敏不该把调试价值也删光"
    );

    let token = TokenKeyring::single(fx::SIGNING_KEY.to_vec())
        .issue(
            datazen_runtime::gateway::SubmissionOperation::ExecuteInSession,
            Some(datazen_runtime::connection::Counter::new(1)),
            fx::TOKEN_ISSUED_AT_NANOS,
            fx::TOKEN_TTL_NANOS,
        )
        .unwrap_or_else(|err| panic!("签发应成功：{err:?}"));

    let registry = datazen_runtime::gateway::GrantRegistry::shared();
    let guard = datazen_runtime::gateway::SubmissionTokenGuard::shared(keyring.shared(), registry);
    let admission = guard
        .admit(
            &token,
            fx::TOKEN_ISSUED_AT_NANOS + 1,
            Some(datazen_runtime::connection::Counter::new(1)),
        )
        .unwrap_or_else(|err| panic!("真令牌应被受理：{err:?}"));

    for (label, rendered) in [
        ("SubmissionTokenGuard", format!("{guard:?}")),
        ("TokenAdmission", format!("{admission:?}")),
    ] {
        assert!(
            !rendered.contains(&token),
            "{label} 的 Debug 泄出了令牌原文"
        );
        assert!(
            !rendered.contains(std::str::from_utf8(fx::SIGNING_KEY).expect("夹具密钥是 ASCII")),
            "{label} 的 Debug 泄出了密钥原文"
        );
        // MAC 段是「凭它就能原样复现令牌」的那一段，脱敏输出里同样不许出现。
        let mac = token.rsplit('.').next().unwrap_or_default();
        assert!(!rendered.contains(mac), "{label} 的 Debug 泄出了 MAC 段");
    }
}

/// 断言 3 的补充：回执与受理结果里也没有令牌原文。
#[tokio::test]
async fn the_receipt_and_the_acceptance_stay_free_of_the_token() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let acceptance = h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await
        .unwrap_or_else(|err| panic!("受理应成功：{err:?}"));
    let rendered = format!("{acceptance:?}");
    assert!(!rendered.contains(&token), "受理结果里出现了令牌原文");
    assert!(
        rendered.contains(&acceptance.execution_id().as_str()),
        "受理结果里本来就有回执"
    );
}

/// 门禁自己得先证明有效：栽一个泄漏，探测器必须响。
#[test]
fn the_detector_catches_a_planted_leak() {
    const PLANTED: &str = r#"
        fn 落盘(seq: &str, key: &str) {
            let mut file = std::fs::File::create(seq).unwrap();
            file.write_all(key.as_bytes()).unwrap();
        }
    "#;
    let hits: Vec<&str> = FORBIDDEN
        .iter()
        .filter(|(_, needle)| PLANTED.contains(needle))
        .map(|(name, _)| *name)
        .collect();
    assert!(
        !hits.is_empty(),
        "探测器对这段明显落盘的代码居然一声不吭：证据本身失效了"
    );
    assert!(detect(PLANTED).is_some(), "detect() 对栽进去的泄漏没有反应");
}

// ─────────────────── 第 4 类证据：错误与记录投影不泄令牌 ───────────────────
//
// 前三条证据管的是「不落盘」，这一条管的是「不外泄」。两者不可互相替代：
// 一个 `tracing::error!("{:?}", err)` 或者一句日志里的 `{:#?}`，就足以把
// 一枚**签名令牌**写进日志，而日志是会落盘的——那正好绕开第 1 条证据的全部能力。
//
// 令牌串 `cm70.<keyVersion>.<hex(payload)>.<hex(MAC)>` 里有两段是凭据：
// `hex(payload)` 让攻击者拿到签发时刻与到期时刻（可重放到窗口内），
// `hex(MAC)` 是 HMAC 的一部分。所以判据不只是「不出现整串令牌」，
// 而是**整串与两段各自都不得出现**——只挡整串的话，把 MAC 段截掉再打一遍就漏了。

/// 一枚令牌里两段凭据的十六进制文本。
///
/// 段数不对时**只**报静态文案：这一行的失败发生在任何泄漏检查**之前**，此刻 `parts`
/// 就是那枚真令牌的四个片段，插进 panic 文本等于让该助手的任意一次失败都把四段真凭据
/// 打进 CI 日志。段数不对说明夹具坏了，不是运行期可观测的行为，静态文案已足够定位。
fn credential_segments(token: &str) -> Vec<String> {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(
        parts.len(),
        4,
        "令牌必须是四段（段数错说明夹具坏了，不回显以免带出凭据）"
    );
    vec![parts[2].to_owned(), parts[3].to_owned()]
}

/// 断言一段渲染文本里既没有整串令牌，也没有它两段凭据。
///
/// `rendered` 正是**被怀疑**装了凭据的那段文本，`token` 本身就是凭据——两者都绝不进
/// panic 文本。失败文案只说「哪个出口、哪一类凭据、第几段序号」，不说「长什么样」，
/// 否则这条断言自己就成了泄漏路径。
fn assert_no_token_material(what: &str, rendered: &str, token: &str) {
    assert!(!rendered.contains(token), "{what} 里出现了令牌原文");
    for (index, segment) in credential_segments(token).into_iter().enumerate() {
        assert!(
            !rendered.contains(&segment),
            "{what} 里出现了令牌的一段凭据（段序号 {index}）"
        );
    }
}

/// 把 `Display` 与 `to_persistable_json()` 两条投影一起交给断言。
///
/// 两条都要查，因为它们服务不同的出口：`Display` 进日志与 panic 文本，
/// `to_persistable_json` 进 IPC 载荷与前端。少查一条就等于漏掉一整个出口面。
fn assert_both_projections_are_clean(what: &str, error: &GatewayError, token: &str) {
    assert_no_token_material(&format!("{what} 的 Display"), &error.to_string(), token);
    assert_no_token_material(
        &format!("{what} 的 JSON 投影"),
        &error.to_persistable_json().to_string(),
        token,
    );
}

/// 编译期闸：`GatewayError` 每加一个变体，这里就编译不过。
///
/// **没有 `_ =>` 兜底**，这是刻意的：兜底会让新变体悄无声息地绕过投影检查，
/// 而新变体正是最可能携带请求派生材料的那一个。想加变体的人必须在这里表态。
fn variant_is_covered(error: &GatewayError) -> bool {
    match error {
        // 7 个变体，逐个表态。
        GatewayError::Runtime(_) => true,
        GatewayError::PermissionDenied { .. } => true,
        GatewayError::IdempotencyVerificationRequired { .. } => true,
        GatewayError::IdempotencyConflict { .. } => true,
        GatewayError::IdempotencyPersistFailed { .. } => true,
        GatewayError::SubmissionTokenRejected { .. } => true,
        GatewayError::InvalidRequest { .. } => true,
    }
}

// 「网关错误投影面」这一组测试拆到这里，拆分理由见子模块文件头。
// 集成测试的 crate 根不按文件名开子目录（只有 `lib.rs` / `main.rs` / `mod.rs` 才开），
// 所以这里必须显式写 `#[path]`。
#[path = "cm70_no_disk/error_projection.rs"]
mod error_projection;

/// 变体表逐个表态：`variant_is_covered` 缺哪个变体就编译不过，这里再把
/// 「已表态」这件事在运行期复述一遍——两道闸的失效方式不同，值这道。
#[test]
fn every_gateway_error_variant_is_accounted_for() {
    let variants = [
        GatewayError::Runtime(RuntimeError::SessionLost("se".to_owned())),
        GatewayError::PermissionDenied {
            action: GatewayAction::Execute,
            reason: "permissionRevoked",
        },
        GatewayError::IdempotencyVerificationRequired {
            message: "needsCheck".to_owned(),
        },
        GatewayError::IdempotencyConflict {
            existing: ExecutionId::new("exe_probe".to_owned()),
            incoming: "0123456789abcdef".to_owned(),
        },
        GatewayError::IdempotencyPersistFailed {
            message: "ledgerDown".to_owned(),
        },
        GatewayError::SubmissionTokenRejected {
            reason: "submissionTokenExpired",
        },
        GatewayError::InvalidRequest {
            reason: "alreadyDispatched",
        },
    ];
    for error in &variants {
        assert!(
            variant_is_covered(error),
            "{error:?} 没被 variant_is_covered 表态：新变体必须逐个决策"
        );
        assert_eq!(
            error.to_persistable_json()["kind"].as_str().is_some(),
            true,
            "{error:?} 必须有机器可读的 kind"
        );
    }
    assert_eq!(
        variants.len(),
        7,
        "变体数变了就说明有变体没进这张表：enum GatewayError 与本表必须同步更新"
    );
}

/// `Runtime` 变体里裹的是驱动原样透传的错误，那段文本**不是**网关自己能管的。
///
/// 所以边界写在这里，而不是含糊过去：
/// - **网关自己产生的错误**里不许有凭据（上面各条用例查的就是这个）；
/// - **驱动传上来的文本**在 `Display` 里保持透明（排障要看得见原话），
///   但落盘的 `to_persistable_json` 只投影 `reason` / `api_code`，把消息文本丢掉。
/// 这条断言把这个边界钉死，免得有人顺手把 JSON 也放行成透明文本。
#[test]
fn the_runtime_variant_drops_driver_text_in_the_persisted_projection() {
    const DRIVER_TEXT: &str = "driver said: connection reset by peer";
    let error = GatewayError::Runtime(RuntimeError::SessionLost(DRIVER_TEXT.to_owned()));
    assert!(
        error.to_string().contains(DRIVER_TEXT),
        "Display 必须保持透明，排障要看得见驱动原话"
    );
    let json = error.to_persistable_json().to_string();
    assert!(
        !json.contains("connection reset"),
        "落盘投影不该携带驱动原文：{json}"
    );
    assert!(
        json.contains("sessionLost") || json.contains("reason"),
        "投影仍须留下机器可读的分类：{json}"
    );
}

/// 把 `Ok` 变成带上下文的失败断言。
fn refused<T>(result: Result<T, GatewayError>, want: &str) -> GatewayError {
    match result {
        Ok(_) => panic!("{want}，实际受理成功"),
        Err(error) => error,
    }
}
