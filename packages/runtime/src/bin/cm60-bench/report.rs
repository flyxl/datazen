//! §11.5 产物落盘与 §11.6 的「三段即可重跑」硬要求。
//!
//! `fake-runtime-fixtures.md` §11.5 `:576-586` 规定基准入口要写两个文件：
//!
//! - `target/bench/cm60-raw-<ts>.json` —— 原始计时（每请求两段纳秒值、轮次、并发度）
//!   加上 `environment` / `build` / `journal` 三段摘要；
//! - `target/bench/cm60-summary-<ts>.json` —— 分位数与判定结论。
//!
//! §11.6 `:591` 要求 `environment` + `build` + `raw` 三段**齐备**，否则照着产物
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

/// 机器与运行方式。**这是 §11.6 重跑三段之一**：不记环境，重跑就等于重猜。
#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    /// 判据指定的可复现性锚点（4 vCPU / 8 GiB）。
    pub criterion_vcpus: u32,
    pub criterion_memory_bytes: u64,
    /// 实测值。基准自己拿不到 CPU 数，只能由入口从外部注入。
    pub measured_vcpus: u32,
    pub measured_memory_bytes: u64,
    pub os: String,
    pub arch: String,
    /// 虚拟时间驱动(fake 10 毫秒)意味着 8 并发共享一个线程，实测值是单核下界。
    pub runtime_threads: &'static str,
    /// §11.1 `:548` 禁止任何出站 socket；此处声明而不是靠「跑通了」来推定。
    pub outbound_socket_policy: &'static str,
    pub tokio_time_paused: bool,
}

/// 构建口径。**§11.6 重跑三段之二**：release 与否直接决定数字含义。
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
    /// 每请求两段原始纳秒值 + 轮次 + 轮内序号。§11.3 要求先按请求求和再取分位，
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
}

#[derive(Debug, Clone, Serialize)]
pub struct RoundRecord<'a> {
    pub round: usize,
    pub concurrency: usize,
    pub n: usize,
    pub p95_nanos: Option<u64>,
    pub percentiles: &'a crate::outcome::Percentiles,
    pub failures: usize,
    pub queued: usize,
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
        .map(|d| {
            u128::try_from(d.as_millis())
                .unwrap_or(u128::MAX)
                .to_string()
        })
        .unwrap_or_else(|_| TIMESTAMP_FALLBACK.to_owned())
}

/// 组装两段环境/构建记录。`measured_*` 由入口注入：基准进程读不到 CPU 拓扑。
pub fn environment(measured_vcpus: u32, measured_memory_bytes: u64) -> Environment {
    Environment {
        criterion_vcpus: SPEC_VCPUS,
        criterion_memory_bytes: SPEC_MEMORY_BYTES,
        measured_vcpus,
        measured_memory_bytes,
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
        criterion_source:
            "docs/architecture/platform/connection-management.md:1235-1239 (CM-60); \
                           bench spec docs/architecture/platform/fake-runtime-fixtures.md 11.1-11.6",
    }
}

/// `option_env!` 在构建期固化，`std::process::Command` 在生产路径上 spawn 子进程
/// 不合适——基准全程零出站 socket 是硬要求，这里用构建期变量而不是运行期探测。
fn rustc_version() -> Option<String> {
    option_env!("DZ_CM60_RUSTC_VERSION").map(str::to_owned)
}

fn plan_record(plan: BenchPlan, conforms: bool) -> PlanRecord {
    PlanRecord {
        warmup: plan.warmup,
        per_round: plan.per_round,
        rounds: plan.rounds,
        concurrency: plan.concurrency,
        fake_command_millis: u64::try_from(plan.fake_command.as_millis()).unwrap_or(u64::MAX),
        conforms_to_spec: conforms,
    }
}

fn round_record(round: &RoundOutcome) -> RoundRecord<'_> {
    RoundRecord {
        round: round.round,
        concurrency: round.concurrency,
        n: round.n(),
        p95_nanos: round.p95_nanos(),
        percentiles: &round.percentiles,
        failures: round.failures(),
        queued: round.outcome.queued,
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

    let env = environment(run.measured_vcpus, run.measured_memory_bytes);
    let build = build();
    let stamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let plan = plan_record(run.plan, run.conforms_to_spec);
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
    use crate::outcome::{ExecutionJournal, Percentiles, SampleOutcome};
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
        assert_eq!(env.measured_vcpus, 8);
        assert_eq!(env.measured_memory_bytes, 17_179_869_184);
    }

    #[test]
    fn a_faster_box_is_still_recorded_as_a_deviation() {
        let env = environment(8, 17_179_869_184);
        let deviates = env.measured_vcpus != env.criterion_vcpus
            || env.measured_memory_bytes != env.criterion_memory_bytes;
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
            plan: plan_record(run.plan, run.conforms_to_spec),
            journal: &run.journal,
            rounds: run.rounds.iter().map(round_record).collect(),
            raw: &run.raw,
            notes: &run.notes,
        };
        let text = serde_json::to_string(&artifact).expect("产物应可序列化");
        assert!(text.contains("\"environment\""));
        assert!(text.contains("\"build\""));
        assert!(text.contains("\"raw\""));
        assert_eq!(
            artifact.raw.len(),
            run.rounds.iter().map(RoundOutcome::n).sum::<usize>()
        );
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
        let outcome = write(
            &run_bench(quick(1, 2)).expect("最小计划应可跑"),
            Some(std::path::Path::new(&tempdir())),
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
        let _ = std::fs::remove_dir_all(&tempdir());
    }

    #[test]
    fn percentiles_and_counts_are_reported_independently() {
        // 样本集为空时，分位数必须是 None 而不是 0；计数走另一条路。
        let empty = Percentiles::of(&[]);
        assert!(empty.is_empty());
        assert_eq!(empty.p95_nanos, None);
        let outcome = SampleOutcome::default();
        assert_eq!(outcome.failures(), 0);
        let _ = ExecutionJournal {
            admitted: 0,
            dispatched: 0,
            completed: 0,
            outstanding_at_end: 0,
            session_view_calls: 0,
            execute_calls: 0,
            cancel_calls: 0,
            close_calls: 0,
            events_emitted: 0,
            events_projected: 0,
            execution_records_retained: 0,
            projection: crate::driver::ProjectionReport::default(),
            driver_round_trip: crate::driver::DriverRoundTrip::empty(),
            driver_round_trip_median_nanos: None,
            dropped_round_trips: 0,
            permit_reconciliation: crate::outcome::PermitReconciliation {
                gateway_ledger_balanced: true,
                budget_permit_ledger_carrier: String::new(),
                budget_permit_ledger_command: String::new(),
            },
        };
    }

    #[test]
    fn the_default_artifact_dir_is_under_target_not_the_system_temp_dir() {
        assert_eq!(DEFAULT_ARTIFACT_DIR, "target/bench");
        assert!(!DEFAULT_ARTIFACT_DIR.starts_with("/tmp"));
    }

    fn tempdir() -> String {
        std::env::temp_dir()
            .join(format!("cm60-bench-report-{}", crate::report::timestamp()))
            .to_string_lossy()
            .into_owned()
    }
}
