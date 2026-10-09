//! 产物落盘与「三段即可重跑」的硬要求。
//!
//! 基准入口要写两个文件：
//!
//! - `target/bench/cm60-raw-<ts>.json` —— 原始计时（每请求两段纳秒值、轮次、并发度）
//!   加上 `environment` / `build` / `journal` 三段摘要；
//! - `target/bench/cm60-summary-<ts>.json` —— 分位数与判定结论。
//!
//! `environment` + `build` + `raw` 三段必须**齐备**，否则照着产物
//! 重跑的人还得回头猜环境。这里让三段的缺一在类型上就写不出来：raw 文件是
//! [`RawArtifact`]，它的三个字段都不是 `Option`。
//!
//! 路径按 spec 固定落在 `target/bench/`（`target/` 已被 gitignore），而不是
//! 系统临时目录——`/tmp` 只放构建与测试的**日志**。两个文件名共用一个时间戳，
//! 这样一对产物不会错配。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::outcome::{BenchRun, RoundOutcome, Verdict};
use crate::plan::{BenchPlan, GATE_P95_NANOS, SPEC_FAKE_COMMAND, SPEC_MEMORY_BYTES, SPEC_VCPUS};

/// 产物根目录，相对仓库根。`--out` 可覆盖。
const DEFAULT_ARTIFACT_DIR: &str = "target/bench";
const RAW_PREFIX: &str = "cm60-raw-";
const SUMMARY_PREFIX: &str = "cm60-summary-";
/// 时间戳文件名安全字符集之外的兜底：只允许 ASCII 数字与 `-`。
const TIMESTAMP_FALLBACK: &str = "00000000";

/// 机器与运行方式。**这是重跑三段之一**：不记环境，重跑就等于重猜。
#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    /// 判据指定的可复现性锚点（4 vCPU / 8 GiB）。
    pub criterion_vcpus: u32,
    pub criterion_memory_bytes: u64,
    /// **声明值**（命令行注入），不是探测值——所以字段名就叫 declared：
    /// 产物里写着一个数，不等于这个数被测量过。调用方不填就是 0（未声明）。
    pub declared_vcpus: u32,
    pub declared_memory_bytes: u64,
    /// 唯一真正探测出来的数：`std::thread::available_parallelism()` 的结果，
    /// 探不到时为 `None`（而不是 0，0 会被误读成「探测到 0 核」）。
    /// 内存**没有**进程内探针（禁止起子进程/开 socket 读 `/proc` 之类），
    /// 所以内存那一栏只能保持声明值形态，并由 `notes` 明说这一点。
    pub detected_parallelism: Option<u32>,
    pub os: String,
    pub arch: String,
    /// 虚拟时间驱动(fake 10 毫秒)意味着 8 并发共享一个线程，实测值是单核下界。
    pub runtime_threads: &'static str,
    /// 禁止任何出站 socket；此处声明而不是靠「跑通了」来推定。
    pub outbound_socket_policy: &'static str,
    pub tokio_time_paused: bool,
}

/// 构建口径。**重跑三段之二**：release 与否直接决定数字含义。
#[derive(Debug, Clone, Serialize)]
pub struct BuildRecord {
    pub profile: &'static str,
    pub debug_assertions: bool,
    pub fake_command_millis: u64,
    pub gate_p95_nanos: u64,
    pub rustc_version: String,
    /// 压测半与延迟半分开的证明。
    pub pressure_harness: &'static str,
    pub benchmark_entry: &'static str,
    pub criterion_source: &'static str,
    /// crate 声明的 feature 与本次是否启用。同一份代码在不同 feature 下不是同一份代码，
    /// 缺了它，「照着产物重跑」就漏了一半输入。
    pub features: String,
    /// 依赖锁文件的字节数与摘要。`Cargo.lock` 不入库比对，但产物得留一个指纹，
    /// 否则换一次依赖解析，重跑出来的数就没人认了。
    ///
    /// 摘要用 **FNV-1a-64**：它不是加密哈希，只用于「同一份锁文件 vs 换过的锁文件」这一档
    /// 区分，不承担任何抗碰撞职责，写在这里是为了不让读产物的人误以为它是密码学摘要。
    pub lock_bytes: usize,
    pub lock_digest_fnv1a64: String,
    pub lock_digest_algorithm: &'static str,
}

/// 原始产物。三段齐备，不允许缺一。
#[derive(Debug, Clone, Serialize)]
pub struct RawArtifact<'a> {
    pub schema: &'static str,
    pub captured_at_unix_nanos: u64,
    pub environment: &'a Environment,
    pub build: &'a BuildRecord,
    pub plan: PlanRecord,
    pub journal: &'a crate::outcome::ExecutionJournal,
    pub rounds: Vec<RoundRecord<'a>>,
    /// 每请求两段原始纳秒值 + 轮次 + 轮内序号。要求先按请求求和再取分位，
    /// 所以这里存的是两段的分段值，求和可复算，不存已聚合的分位数。
    pub raw: &'a [crate::outcome::RawSample],
    pub notes: &'a [String],
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlanRecord {
    pub warmup: usize,
    pub per_round: usize,
    pub rounds: usize,
    pub concurrency: usize,
    pub fake_command_millis: u64,
    pub conforms_to_spec: bool,
    /// 非 0 表示这次运行开了故障注入：产物**不是**判据证据，只是口径自证。
    pub inject_failure_every: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoundRecord<'a> {
    pub round: usize,
    pub concurrency: usize,
    /// N = 获准且未排队的请求数（含随后失败的）。
    pub n: usize,
    /// 分位数的真实输入条数。**它和 `n` 不是同一个数**，差值就是 `unmeasured_failures`。
    /// 失败样本不得从统计里消失，所以这两个数必须并排写出来：
    /// 只写 `n` 会让人以为 N 条请求全都测到了。
    pub measured: usize,
    /// 分位数**真正吃进去**的条数（分位数的输入条数必须是独立一列）。它与
    /// `measured` 恒等；一旦不等，说明算 p95 的向量被做短了，而只写 `measured` 的话
    /// 产物里没有任何一列会矛盾（`measured` 数的是样本仓库，这一列数的是分位数真正
    /// 吃进去的向量，两者是不同来源）。
    pub percentile_input: usize,
    /// 计入 `n` 却打不出第二段真实时长的请求数。**非 0 即门禁不成立**；
    /// 这些请求**没有**被补一个构造值进分位数。
    pub unmeasured_failures: usize,
    pub p95_nanos: Option<u64>,
    pub percentiles: &'a crate::outcome::Percentiles,
    /// 失败数。要求它**与占比**一起输出。
    pub failures: usize,
    /// 失败占比（失败 / 提交）。分母为 0 时是 `None`（未知），不是 `0.0`——
    /// `0.0` 会被读成「测过，失败率 0」。
    pub failure_ratio: Option<f64>,
    pub queued: usize,
    /// 按原因的拒绝/失败计数。键来自 `runner::rejection_label`
    /// （`runtime:{reason}` / `permission:{action}` / `idempotency:{kind}` / `invalid:{reason}`）。
    /// 之前这些数只在内存里汇过，**从不写进产物**——边界的死数据，等于没统计。
    pub rejections: &'a std::collections::BTreeMap<String, usize>,
    pub wall_time_nanos: u64,
    pub event_projection: &'a crate::driver::ProjectionReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryArtifact<'a> {
    pub schema: &'static str,
    pub captured_at_unix_nanos: u64,
    pub environment: &'a Environment,
    pub build: &'a BuildRecord,
    pub plan: PlanRecord,
    pub verdict: Verdict,
    pub rounds: Vec<RoundRecord<'a>>,
    pub journal: &'a crate::outcome::ExecutionJournal,
    /// 预热轮单列：它的样本照采但不进任何分位数。
    pub warmup: RoundRecord<'a>,
    pub raw_artifact_file: String,
    pub summary_artifact_file: String,
    pub notes: &'a [String],
}

/// 两个文件都写成功后的回执。
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub raw_path: PathBuf,
    pub summary_path: PathBuf,
    pub raw_bytes: usize,
    pub summary_bytes: usize,
}

impl WriteOutcome {
    pub fn paths(&self) -> [&Path; 2] {
        [self.raw_path.as_path(), self.summary_path.as_path()]
    }
}

/// 取一个文件名安全的时间戳（UTC 毫秒，失败时退回全零）。
pub fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| TIMESTAMP_FALLBACK.to_owned())
}

/// 组装环境记录。
///
/// `declared_*` 由入口注入，是**声明**不是测量：基准进程不去读 `/proc`、不起子进程
/// （禁止出站 socket，且没有额外依赖），所以 CPU 数与内存只能由调用方声明。
/// 这里额外探一个 `available_parallelism()`，好让读产物的人至少有一个真数可以对照；
/// 内存没有进程内探针，只能保持声明形态，并在 `notes` 里说明。
pub fn environment(declared_vcpus: u32, declared_memory_bytes: u64) -> Environment {
    Environment {
        criterion_vcpus: SPEC_VCPUS,
        criterion_memory_bytes: SPEC_MEMORY_BYTES,
        declared_vcpus,
        declared_memory_bytes,
        detected_parallelism: std::thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok()),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        runtime_threads: "single-threaded (tokio current_thread + time::pause)",
        outbound_socket_policy:
            "no outbound socket: the tokio runtime is built with enable_time() only (no IO driver) \
             and the fake driver port performs no I/O, so no socket can be opened by construction",
        tokio_time_paused: true,
    }
}

pub fn build() -> BuildRecord {
    BuildRecord {
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        debug_assertions: cfg!(debug_assertions),
        fake_command_millis: u64::try_from(SPEC_FAKE_COMMAND.as_millis()).unwrap_or(u64::MAX),
        gate_p95_nanos: GATE_P95_NANOS,
        rustc_version: rustc_version().unwrap_or_else(|| "unknown".to_owned()),
        pressure_harness: "cargo test -p datazen-runtime --test cm60_pressure_drain",
        benchmark_entry: "cargo run --release -p datazen-runtime --bin cm60-bench",
        criterion_source: "built-in bench criterion: per-round p95 <= 10ms, failures == 0",
        features: features(),
        lock_bytes: WORKSPACE_LOCK.len(),
        lock_digest_fnv1a64: fnv1a64_hex(WORKSPACE_LOCK.as_bytes()),
        lock_digest_algorithm: "FNV-1a 64-bit (non-cryptographic; change fingerprint only)",
    }
}

/// 工作区 `Cargo.lock` 在编译期固化：运行期去读文件系统路径，才是真的「照着产物重跑」吗？
/// 不——是产物自己得带走它依赖的那一份，否则产物和仓库之间没有任何可核对的绑定。
const WORKSPACE_LOCK: &str = include_str!("../../../../../Cargo.lock");

/// crate 声明的 feature 及本次启用情况。
///
/// Cargo 把启用的 feature 以 `CARGO_FEATURE_<名字>` 注入编译期环境，未启用则变量不存在。
/// 这里列出的名字与 `packages/runtime/Cargo.toml` 的 `[features]` 段一一对应，
/// 由 `every_declared_feature_appears_in_the_build_record` 盯着，新增 feature 忘了登记就红。
fn features() -> String {
    format!(
        "default=on (declared as []); test-harness={}",
        if option_env!("CARGO_FEATURE_TEST_HARNESS").is_some() {
            "on"
        } else {
            "off"
        }
    )
}

/// FNV-1a-64 的十六进制摘要。非加密用途，只用来区分「同一份锁文件 / 换过的锁文件」。
fn fnv1a64_hex(bytes: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

/// `option_env!` 在构建期固化，`std::process::Command` 在生产路径上 spawn 子进程
/// 不合适——基准全程零出站 socket 是硬要求，这里用构建期变量而不是运行期探测。
fn rustc_version() -> Option<String> {
    option_env!("DZ_CM60_RUSTC_VERSION").map(str::to_owned)
}

fn plan_record(plan: BenchPlan, conforms: bool, inject_failure_every: u64) -> PlanRecord {
    PlanRecord {
        warmup: plan.warmup,
        per_round: plan.per_round,
        rounds: plan.rounds,
        concurrency: plan.concurrency,
        fake_command_millis: u64::try_from(plan.fake_command.as_millis()).unwrap_or(u64::MAX),
        conforms_to_spec: conforms,
        inject_failure_every,
    }
}

fn round_record(round: &RoundOutcome) -> RoundRecord<'_> {
    RoundRecord {
        round: round.round,
        concurrency: round.concurrency,
        n: round.n(),
        measured: round.measured(),
        percentile_input: round.percentile_input(),
        unmeasured_failures: round.unmeasured_failures(),
        p95_nanos: round.p95_nanos(),
        percentiles: &round.percentiles,
        failures: round.failures(),
        failure_ratio: round.failure_ratio(),
        queued: round.outcome.queued,
        rejections: &round.rejections,
        wall_time_nanos: round.wall_time_nanos,
        event_projection: &round.event_projection,
    }
}

/// 落盘。`out` 为 `Some` 时当作**产物根目录**（不是文件名），保证 raw 与
/// summary 仍然成对、同时间戳。
pub fn write(run: &BenchRun, out: Option<&Path>) -> Result<WriteOutcome, BenchError> {
    let dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ARTIFACT_DIR));
    std::fs::create_dir_all(&dir)
        .map_err(|error| BenchError::Io(format!("无法创建产物目录 {}: {error}", dir.display())))?;

    let stamp = timestamp();
    let raw_path = dir.join(format!("{RAW_PREFIX}{stamp}.json"));
    let summary_path = dir.join(format!("{SUMMARY_PREFIX}{stamp}.json"));

    let env = environment(run.declared_vcpus, run.declared_memory_bytes);
    let build = build();
    let stamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let plan = plan_record(run.plan, run.conforms_to_spec, run.inject_failure_every);
    let rounds: Vec<RoundRecord<'_>> = run.rounds.iter().map(round_record).collect();

    let raw_artifact = RawArtifact {
        schema: "datazen.cm60-bench.raw.v1",
        captured_at_unix_nanos: stamp_nanos,
        environment: &env,
        build: &build,
        plan,
        journal: &run.journal,
        rounds: rounds.clone(),
        raw: &run.raw,
        notes: &run.notes,
    };

    let summary_artifact = SummaryArtifact {
        schema: "datazen.cm60-bench.summary.v1",
        captured_at_unix_nanos: stamp_nanos,
        environment: &env,
        build: &build,
        plan,
        verdict: run.verdict(),
        rounds,
        journal: &run.journal,
        warmup: round_record(&run.warmup),
        raw_artifact_file: raw_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        summary_artifact_file: summary_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        notes: &run.notes,
    };

    let raw_text = serde_json::to_string_pretty(&raw_artifact)
        .map_err(|error| BenchError::Serialize(format!("raw 产物序列化失败: {error}")))?;
    let summary_text = serde_json::to_string_pretty(&summary_artifact)
        .map_err(|error| BenchError::Serialize(format!("summary 产物序列化失败: {error}")))?;
    std::fs::write(&raw_path, raw_text.as_bytes())
        .map_err(|error| BenchError::Io(format!("无法写入 {}: {error}", raw_path.display())))?;
    std::fs::write(&summary_path, summary_text.as_bytes())
        .map_err(|error| BenchError::Io(format!("无法写入 {}: {error}", summary_path.display())))?;

    Ok(WriteOutcome {
        raw_path,
        summary_path,
        raw_bytes: raw_text.len(),
        summary_bytes: summary_text.len(),
    })
}

/// 产物相关错误。与 runner 的运行错误分开，因为处置方式不同（落盘失败不用重跑基准）。
#[derive(Debug)]
pub enum BenchError {
    Io(String),
    Serialize(String),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) | Self::Serialize(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for BenchError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Percentiles, SampleOutcome};
    use crate::plan::DEFAULT_PLAN;
    use crate::runner::run_bench;

    /// 单测不跑完整判据计划：预热保持非零（这样「预热样本被切出去」仍然被覆盖），
    /// 但规模缩到能当单测跑的量级。
    fn quick(rounds: usize, per_round: usize) -> crate::plan::BenchPlan {
        DEFAULT_PLAN
            .with_warmup(4)
            .with_rounds(rounds)
            .with_per_round(per_round)
    }

    #[test]
    fn spec_anchors_are_recorded_verbatim_not_converted() {
        let env = environment(8, 17_179_869_184);
        assert_eq!(env.criterion_vcpus, 4);
        assert_eq!(env.criterion_memory_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(env.declared_vcpus, 8);
        assert_eq!(env.declared_memory_bytes, 17_179_869_184);
        // 唯一真正探测出来的那个数：探不到时是 None，绝不是 0（0 会被读成「探测到 0 核」）。
        assert!(
            env.detected_parallelism.is_none_or(|n| n > 0),
            "探测到的并行度要么是 None，要么是正数"
        );
    }

    #[test]
    fn a_faster_box_is_still_recorded_as_a_deviation() {
        let env = environment(8, 17_179_869_184);
        let deviates = env.declared_vcpus != env.criterion_vcpus
            || env.declared_memory_bytes != env.criterion_memory_bytes;
        assert!(deviates, "8 vCPU / 16 GiB 必须被判为偏离 4 vCPU / 8 GiB");
    }

    #[test]
    fn raw_artifact_carries_all_three_reproducibility_sections() {
        let run = run_bench(quick(1, 2)).expect("最小计划应可跑");
        let env = environment(8, 17_179_869_184);
        let build = build();
        let artifact = RawArtifact {
            schema: "datazen.cm60-bench.raw.v1",
            captured_at_unix_nanos: 0,
            environment: &env,
            build: &build,
            plan: plan_record(run.plan, run.conforms_to_spec, run.inject_failure_every),
            journal: &run.journal,
            rounds: run.rounds.iter().map(round_record).collect(),
            raw: &run.raw,
            notes: &run.notes,
        };
        let text = serde_json::to_string(&artifact).expect("产物应可序列化");
        assert!(text.contains("\"environment\""));
        assert!(text.contains("\"build\""));
        assert!(text.contains("\"raw\""));
        // 三段齐备是类型强制的（Environment/Build 都非 Option），这里额外钉住
        // `features` / `lock_digest_fnv1a64` 确实出现在序列化结果里，而不是被
        // `skip_serializing` 之类的手段抹掉。
        assert!(text.contains("\"features\""));
        assert!(text.contains("\"lock_digest_fnv1a64\""));
        // raw 条数必须等于**分位数的真实输入条数**。写 N 会在存在未测出样本时虚报：
        // N 里含打不出第二段终点的那批，它们根本没有样本。
        assert_eq!(
            artifact.raw.len(),
            run.rounds.iter().map(RoundOutcome::measured).sum::<usize>()
        );
        for round in &run.rounds {
            assert_eq!(
                round.measured() + round.unmeasured_failures(),
                round.n(),
                "N = 测到 + 未测出 必须恒等，否则产物的三个数对不上"
            );
        }
    }

    #[test]
    fn the_raw_block_stores_two_segments_not_a_precomputed_sum() {
        let run = run_bench(quick(1, 3)).expect("最小计划应可跑");
        let first = run.raw.first().expect("应至少有一个样本");
        assert_eq!(
            first.gateway_nanos.saturating_add(first.registration_nanos),
            crate::outcome::total_nanos(first)
        );
    }

    #[test]
    fn every_written_sample_carries_its_own_round_and_ordinal() {
        let run = run_bench(quick(2, 3)).expect("最小计划应可跑");
        let mut seen = std::collections::BTreeSet::new();
        for entry in &run.raw {
            assert!(
                seen.insert((entry.round, entry.ordinal)),
                "轮次/序号必须唯一"
            );
            assert!((1..=3).contains(&entry.ordinal));
            assert!((1..=2).contains(&entry.round));
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn a_run_without_rounds_is_refused_before_any_file_is_touched() {
        let denied = run_bench(DEFAULT_PLAN.with_rounds(0));
        assert!(denied.is_err(), "零轮基准必须被拒绝，而不是产出一份空报告");
    }

    #[test]
    fn the_summary_and_the_raw_artifact_are_a_matched_pair() {
        // 取一次、存下来：清理必须针对**同一个**目录。早先这里调了两次
        // `tempdir()`，只有当它按毫秒复用名字时两次才相等；改成每次唯一之后
        // 第二次会指向一个从没写过的空目录，于是清理静默失效、目录留在 temp 里。
        let dir = tempdir();
        let outcome = write(
            &run_bench(quick(1, 2)).expect("最小计划应可跑"),
            Some(std::path::Path::new(&dir)),
        )
        .expect("应可落盘");
        let [raw, summary] = outcome.paths();
        let stamp = raw
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(RAW_PREFIX))
            .expect("raw 文件名应带时间戳");
        assert_eq!(
            summary
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix(SUMMARY_PREFIX)),
            Some(stamp),
            "两个产物必须共用同一个时间戳，否则一对产物会错配"
        );
        assert!(outcome.raw_bytes > 0 && outcome.summary_bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 样本集为空时，分位数必须是 `None` 而不是 0；计数走另一条路。
    /// 顺手钉住：分位数为空**不能**被当成「这一轮没失败」。
    #[test]
    fn percentiles_and_counts_are_reported_independently() {
        let empty = Percentiles::of(&[]);
        assert!(empty.is_empty());
        assert_eq!(empty.p95_nanos, None);
        let outcome = SampleOutcome::default();
        assert_eq!(outcome.failures(), 0);
        // 空样本 + 没有失败是一条自洽的组合；空样本 + 有失败就必须靠 unmeasured_failures 暴露。
        let run = run_bench(quick(1, 4)).expect("最小计划应可跑");
        let round = run.rounds.first().expect("应有一轮");
        assert!(round.measured() > 0, "无注入时每条获准请求都该测到");
        assert_eq!(round.unmeasured_failures(), 0);
        assert!(!round.sample_count_mismatch());
    }

    /// 按原因的拒绝/失败分类必须**落进产物**。
    ///
    /// 之前 `RoundOutcome` 在内存里汇了一份 `rejections`，summary 却只写 `failures`
    /// 一个总数——分布从头到尾没有任何一列落过盘（已核对冻结产物
    /// `cm60-summary-1791128233852.json`：`rounds[0]` 里确实没有 `rejections`）。
    /// 「拒绝有界」是本判据的一半，而读产物的人只拿得到一个和，猜不出是哪一类在涨。
    #[test]
    fn rejections_by_reason_reach_the_artifact() {
        let mut run = run_bench(quick(1, 2)).expect("最小计划应可跑");
        run.rounds[0]
            .rejections
            .insert("runtime:invariantBroken".to_string(), 3);
        run.rounds[0]
            .rejections
            .insert("permission:command".to_string(), 1);
        let dir = tempdir();
        let outcome = write(&run, Some(std::path::Path::new(&dir))).expect("应可落盘");
        let paths = outcome.paths();
        let summary = paths[1];
        let text = std::fs::read_to_string(summary).expect("summary 应可读");
        let json: serde_json::Value = serde_json::from_str(&text).expect("summary 应是 JSON");
        let rejections = json["rounds"][0]["rejections"]
            .as_object()
            .expect("每轮必须带 rejections 列");
        assert_eq!(rejections["runtime:invariantBroken"], 3);
        assert_eq!(rejections["permission:command"], 1);
        // 分位数的输入条数也得在产物里：不然读产物的人无从判断 p95 吃的是哪几个数。
        assert_eq!(
            json["rounds"][0]["percentile_input"], json["rounds"][0]["measured"],
            "p95 的输入条数与样本条数必须在产物里并排可见"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 漂移守卫：`build()` 里硬写了 feature 名，`Cargo.toml` 加了新 feature 而这里
    /// 没登记时，产物就会少记一半构建输入——而这种缺失从产物本身看不出来。
    #[test]
    fn every_declared_feature_appears_in_the_build_record() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("应能读到本 crate 的 Cargo.toml");
        let declared: Vec<&str> = manifest
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("[features]"))
            .skip(1)
            .take_while(|line| !line.trim_start().starts_with('['))
            .filter_map(|line| {
                let line = line.split('#').next().unwrap_or_default().trim();
                line.split_once('=').map(|(name, _)| name.trim())
            })
            .filter(|name| !name.is_empty())
            .collect();
        assert!(
            !declared.is_empty(),
            "解析不到 [features] 段——守卫本身失效了，那比缺登记更危险"
        );
        let recorded = build().features;
        for name in &declared {
            assert!(
                recorded.contains(name),
                "Cargo.toml 声明了 feature `{name}`，但产物 build.features 没记它：{recorded}"
            );
        }
    }

    /// 锁文件摘要是非加密的：算法名必须写在产物里，免得有人拿它当完整性凭据。
    #[test]
    fn the_lock_digest_is_labelled_as_non_cryptographic() {
        let build = build();
        assert!(build.lock_bytes > 0);
        assert_eq!(
            build.lock_digest_fnv1a64.len(),
            16,
            "16 位十六进制 = 64 bit"
        );
        assert!(build.lock_digest_algorithm.contains("non-cryptographic"));
        // 同一份锁文件，同一个摘要。
        assert_eq!(
            build.lock_digest_fnv1a64,
            fnv1a64_hex(WORKSPACE_LOCK.as_bytes())
        );
    }

    #[test]
    fn the_default_artifact_dir_is_under_target_not_the_system_temp_dir() {
        assert_eq!(DEFAULT_ARTIFACT_DIR, "target/bench");
        assert!(!DEFAULT_ARTIFACT_DIR.starts_with("/tmp"));
    }

    /// 每次调用必须拿到**独占**目录。
    ///
    /// 原来只用 `timestamp()`（毫秒）命名，于是同一毫秒里跑的两个测试拿到同一个
    /// 目录，而 `write()` 写进去的文件名又只由时间戳决定——两个测试互相覆盖对方
    /// 的 summary，读到的就是别人的产物，断言报的是 `no entry found for key`。
    /// 这是继承来的竞态：HEAD `fdeff46f` 6/6 并行不红，本轮只因多两个用例把并发
    /// 密度推上去，就 3 次里红 1 次（`--test-threads=1` 6/6 不红）。所以此处按
    /// 「进程内唯一」修：`pid` 挡住跨进程，`seq` 挡住同进程内的并发线程。
    fn tempdir() -> String {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!(
                "cm60-bench-report-{}-{seq}-{}",
                std::process::id(),
                crate::report::timestamp()
            ))
            .to_string_lossy()
            .into_owned()
    }
}
