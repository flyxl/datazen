//! CM-60 性能半（B 半）的基准入口 —— `docs/architecture/platform/fake-runtime-fixtures.md` §11.1–§11.6。
//!
//! **这一半只测「网关附加耗时」**：从鉴权/参数校验完成到派发 driver 的那一段，
//! 加上从 driver completion 到回执/事件状态登记完成的那一段，两段**逐请求先求和**，
//! 再对和取分位数。fake SQL 本身、预算/actor 排队、网络传输**都不在测量窗口里**
//! （`fake-runtime-fixtures.md` §11.3）。
//!
//! 运行：`cargo run --release -p datazen-runtime --bin cm60-bench`
//! （§12 的命令表与 §11.6 的「单独入口，便于 CI 区分超时原因」指的是这一条。）
//!
//! ## 为什么落成 `src/bin/cm60-bench`，而不是 `connection/testing/bench.rs`
//!
//! §2 的模块表把基准列为 `connection/testing/bench.rs`，那行是**过期文本**：
//! §11.6 要求基准不得与功能测试共用同一二进制入口，§12 给出的命令逐字是
//! `cargo run --release -p datazen-runtime --bin cm60-bench`。两处 norm 冲突时以
//! §11.6/§12 为准，所以入口必须是独立的 bin target。
//!
//! 它也**不能**是 `src/bench/` 之类的 lib 模块：那会把基准代码编译进随桌面应用发布的
//! `datazen-runtime` 库里，成为产品二进制的一部分。Cargo 对 bin target 的处理正好相反——
//! `src/bin/<name>/main.rs` 自成一个 crate，基准代码只在该 crate 内链接，发布构建不碰它。
//! 同一份论证见 `connection/testing/mod.rs`：夹具子树（`connection::testing`）继续保持
//! 「只服务 `cargo test`」的门控不变。`Cargo.toml` 里的 `exclude` 也不需要动——
//! 它本来针对的是 `src/connection/testing/**`，与本入口无关。
//!
//! ## 三个不可协商的口径（改任何一条都要先改判据）
//!
//! 1. **release 构建**：debug 档位的 p95 没有任何意义，见 [`reject_debug_build`]。
//! 2. **每一轮都要 ≤ 10 ms**，不是五轮取平均、也不是取最好的那一轮。
//! 3. **nearest-rank 第 `ceil(0.95*N)` 项（1-based）**，不插值。分位数实现只有一份：
//!    `datazen_runtime::latency::nearest_rank_percentile`，唯一调用点在
//!    [`outcome::Percentiles::of`]。在本入口另写一份排序就是口径分裂。
//!
//! ## 失败样本为什么不删
//!
//! `OverheadProbe` 只在**登记段闭合**时才产出一条样本（两段打点都齐全才有）。
//! 所以「删除失败样本」在这里不是「从分位数集合里剔掉失败项」——失败请求压根不会进集合。
//! harness 的做法是**把失败数单列**（[`outcome::RoundOutcome::failures`]）并让判定式要求
//! 它为 0，同时把失败率与排队数一并输出。**空样本集返回 `None`，绝不按 0 毫秒通过。**
//!
//! ## fake 命令的 10 毫秒怎么消费
//!
//! §11.2 写明：两段用 `std::time::Instant` 采样，fake 命令的 10 毫秒由
//! `tokio::time::pause()` + auto-advance 的**虚拟时间**消费，因此不进入测量窗口，
//! 但调度与分配开销仍然是真实的。driver 就按这句话实现（见 [`driver`]）：
//! `tokio::time::sleep(10ms).await` 落在暂停的时钟上，消耗的是虚拟时间。
//! 真实 `std::time::Instant` 仍在两段窗口里正常走动，所以量出来的就是真实开销。
//! **仓库里没有一个 `sleep` 是真的在等**——见 [`driver::FakeDriverPort`]。
//!
//! ## 环境是外部注入的
//!
//! 基准进程读不到 CPU 拓扑（runtime crate 不依赖 `libc`，也不允许为了读一个数字去开
//! 子进程——那会破坏 §11.1「禁止任何出站 socket」的整体交代）。所以实测 vCPU 数与内存字节数
//! 由 `--vcpus` / `--mem-bytes` 显式传入，缺省就是 0 并在产物里如实写 0：
//! **宁可留白，也不填一个猜出来的数。**

pub mod clock;
pub mod driver;
pub mod outcome;
pub mod plan;
pub mod report;
pub mod runner;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use crate::outcome::{BenchRun, RoundOutcome};
use crate::plan::{BenchPlan, DEFAULT_PLAN};

/// §11.1 明确要求 release 构建。debug 档位下 debug_assertions 会让被测路径变样，
/// 量出来的 p95 与判据讨论的不是同一个东西，所以在入口就拒绝。
const USAGE: &str = "\
cm60-bench —— CM-60 性能半（B 半）基准入口

用法：
  cargo run --release -p datazen-runtime --bin cm60-bench -- [选项]

选项（默认值逐字取自 fake-runtime-fixtures.md §11.1）：
  --warmup <N>        预热请求数（默认 1000；预热样本不进任何分位数）
  --rounds <N>        测量轮次（默认 5）
  --per-round <N>     每轮请求数（默认 10000）
  --concurrency <N>   并发任务数（默认 8）
  --fake-ms <N>       fake 命令虚拟耗时毫秒（默认 10）
  --out <DIR>         产物目录（默认 target/bench）
  --vcpus <N>         实测 vCPU 数，写入产物 environment 段（缺省 0＝未采集）
  --mem-bytes <N>     实测内存字节数，写入产物 environment 段（缺省 0＝未采集）
  --help              打印本帮助

退出码：
  0  规格 release 运行，且每轮 p95 ≤ 10 毫秒、失败数 0、事件重复/丢失 0
  1  门禁未通过（有轮超标、有失败、或事件流不干净）
  2  用法、运行或落盘错误
  3  运行完成，但不是 §11.1 的规格计划（不构成「按判据达标」的结论）
";

/// 解析后的命令行。解析失败不是 `Err` 之外的分支：一律走 [`CliError`]，退出码 2。
#[derive(Debug, Clone)]
struct Cli {
    plan: BenchPlan,
    out: Option<PathBuf>,
    measured_vcpus: u32,
    measured_memory_bytes: u64,
}

#[derive(Debug)]
enum CliError {
    Unknown(String),
    Missing(String),
    Invalid { flag: String, value: String },
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(flag) => write!(f, "未知选项 `{flag}`"),
            Self::Missing(flag) => write!(f, "选项 `{flag}` 缺少取值"),
            Self::Invalid { flag, value } => {
                write!(f, "选项 `{flag}` 的取值 `{value}` 不是合法数字")
            }
        }
    }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let cli = match parse(&raw) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("cm60-bench: {error}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    // debug 档位的数字与判据讨论的不是同一个东西。拒绝，而不是打条警告继续。
    if cfg!(debug_assertions) {
        eprintln!(
            "cm60-bench: 这是 debug 构建。§11.1 要求 release 构建，debug 档位的 p95 不可用。\n\
             请改用：cargo run --release -p datazen-runtime --bin cm60-bench -- \\"
        );
        return ExitCode::from(2);
    }

    match execute(&cli) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("cm60-bench: {message}");
            ExitCode::from(2)
        }
    }
}

fn execute(cli: &Cli) -> Result<u8, String> {
    let run = runner::run_bench_on(cli.plan, cli.measured_vcpus, cli.measured_memory_bytes)
        .map_err(|error| format!("基准运行失败: {error}"))?;
    let written = report::write(&run, cli.out.as_deref())
        .map_err(|error| format!("产物落盘失败: {error}"))?;

    print_report(&run, &written);

    Ok(exit_code(&run))
}

/// 退出码就是判定的对外投影，方向不能反。
///
/// - `0`：判定式成立（且计划逐字等于 §11.1），CI 判绿。
/// - `1`：判定式不成立。
/// - `3`：跑完了但计划不是 §11.1 的规格计划，数字不能与判据对话。
///
/// 非规格计划的 3 排在门禁之前：缩小计划去「跑得快一点」不是失败，是不可比。
fn exit_code(run: &BenchRun) -> u8 {
    if !run.conforms_to_spec {
        return 3;
    }
    u8::from(!run.verdict().gate_passed)
}

fn print_report(run: &BenchRun, written: &report::WriteOutcome) {
    println!("cm60-bench —— CM-60 性能半（B 半）");
    println!(
        "计划：预热 {} / 每轮 {} / {} 轮 / 并发 {} / fake {} 毫秒（{}）",
        run.plan.warmup,
        run.plan.per_round,
        run.plan.rounds,
        run.plan.concurrency,
        run.plan.fake_command.as_millis(),
        if run.conforms_to_spec {
            "逐字等于 §11.1 规格"
        } else {
            "**非 §11.1 规格计划**"
        }
    );
    println!(
        "环境：实测 {} vCPU / {} 字节（判据锚点 4 vCPU / 8589934592 字节）",
        run.measured_vcpus, run.measured_memory_bytes
    );
    println!("预热：{} 个样本（不进任何分位数）", run.warmup.n());
    println!("逐轮结果（§11.3：每轮输出 N、p50、p90、p95、p99、最大值、失败数、排队数）：");

    for round in std::iter::once(&run.warmup).chain(run.rounds.iter()) {
        print_round(round, round.round == 0);
    }

    let journal = &run.journal;
    println!(
        "journal：受理 {} / 派发 {} / 完成 {} / 收尾未完成 {} / 执行记录留存 {}",
        journal.admitted,
        journal.dispatched,
        journal.completed,
        journal.outstanding_at_end,
        journal.execution_records_retained
    );
    println!(
        "事件投影：applied {} / 重复 {} / 乱序 {} / 重复块 {} / 丢失 {} / 未绑定 {} / 外来 {}",
        journal.projection.applied,
        journal.projection.duplicates,
        journal.projection.out_of_order,
        journal.projection.duplicate_chunks,
        journal.projection.lost,
        journal.projection.unbound,
        journal.projection.foreign
    );
    println!(
        "driver 往返（含 fake 虚拟耗时）：样本 {} / p95 {} / 最大 {} / 中位 {}",
        journal.driver_round_trip.samples,
        ms(journal.driver_round_trip.p95_nanos),
        ms(journal.driver_round_trip.max_nanos),
        ms(journal.driver_round_trip_median_nanos)
    );
    println!(
        "permit 收支对账：网关侧 {}；预算台账侧由 `{}` 承担（§11.4 压力与延迟分开跑）",
        if journal.permit_reconciliation.gateway_ledger_balanced {
            "平衡"
        } else {
            "**不平衡**"
        },
        journal.permit_reconciliation.budget_permit_ledger_command
    );
    println!("全程真实墙钟：{}", ms(Some(run.wall_time_nanos)));
    println!("口径披露：");
    for note in &run.notes {
        println!("  - {note}");
    }

    println!("产物：");
    println!("  raw     {}", written.raw_path.display());
    println!("  summary {}", written.summary_path.display());

    println!("结论：");
    for line in &run.verdict().conclusions {
        println!("  {line}");
    }
}

fn print_round(round: &RoundOutcome, is_warmup: bool) {
    let tag = if is_warmup {
        "预热".to_owned()
    } else {
        format!("第 {} 轮", round.round)
    };
    println!(
        "  {tag}: N={} p50={} p90={} p95={} p99={} max={} 失败={} 排队={} 墙钟={}",
        round.n(),
        ms(round.percentiles.p50_nanos),
        ms(round.percentiles.p90_nanos),
        ms(round.percentiles.p95_nanos),
        ms(round.percentiles.p99_nanos),
        ms(round.percentiles.max_nanos),
        round.failures(),
        round.outcome.queued,
        ms(Some(round.wall_time_nanos))
    );
}

/// 纳秒 → 毫秒的可读写法。`None` 渲染成 `-` 而不是 `0`：没测到和 0 毫秒不是一回事。
fn ms(nanos: Option<u64>) -> String {
    match nanos {
        Some(value) => format!("{:.3} ms", value as f64 / 1_000_000.0),
        None => "-".to_owned(),
    }
}

fn parse(args: &[String]) -> Result<Cli, CliError> {
    let mut plan = DEFAULT_PLAN;
    let mut out = None;
    let mut measured_vcpus = 0u32;
    let mut measured_memory_bytes = 0u64;

    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1).map(String::as_str).ok_or_else(|| {
            // 末尾的裸值（比如 `--rounds 5 --warmup`）在这里报「缺取值」而不是「未知选项」。
            CliError::Missing(flag.to_owned())
        })?;
        let invalid = |value: &str| CliError::Invalid {
            flag: flag.to_owned(),
            value: value.to_owned(),
        };

        match flag {
            "--warmup" => plan = plan.with_warmup(parse_usize(value, &invalid)?),
            "--rounds" => plan = plan.with_rounds(parse_usize(value, &invalid)?),
            "--per-round" => plan = plan.with_per_round(parse_usize(value, &invalid)?),
            "--concurrency" => plan = plan.with_concurrency(parse_usize(value, &invalid)?),
            "--fake-ms" => {
                let millis = parse_u64(value, &invalid)?;
                plan = plan.with_fake_command(Duration::from_millis(millis));
            }
            "--out" => out = Some(PathBuf::from(value)),
            "--vcpus" => measured_vcpus = parse_u32(value, &invalid)?,
            "--mem-bytes" => measured_memory_bytes = parse_u64(value, &invalid)?,
            other => return Err(CliError::Unknown(other.to_owned())),
        }
        index += 2;
    }

    Ok(Cli {
        plan,
        out,
        measured_vcpus,
        measured_memory_bytes,
    })
}

fn parse_usize(value: &str, invalid: &dyn Fn(&str) -> CliError) -> Result<usize, CliError> {
    value.parse::<usize>().map_err(|_| invalid(value))
}

fn parse_u64(value: &str, invalid: &dyn Fn(&str) -> CliError) -> Result<u64, CliError> {
    value.parse::<u64>().map_err(|_| invalid(value))
}

fn parse_u32(value: &str, invalid: &dyn Fn(&str) -> CliError) -> Result<u32, CliError> {
    value.parse::<u32>().map_err(|_| invalid(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 打印路径的单测要真跑一遍基准，但规格计划（5×10000）属于 `--release` 的职责。
    /// 预热保持非零，这样「预热样本不计入实测轮」这条路径仍然被走到。
    fn quick(rounds: usize, per_round: usize) -> BenchPlan {
        DEFAULT_PLAN
            .with_warmup(4)
            .with_rounds(rounds)
            .with_per_round(per_round)
    }

    /// 不带任何参数必须逐字等于 §11.1 的规格计划：默认值不允许「更保守」。
    #[test]
    fn no_arguments_means_the_spec_plan() {
        let cli = parse(&[]).expect("空参数应可解析");
        assert_eq!(cli.plan, DEFAULT_PLAN);
        assert!(cli.plan.is_spec_plan());
        assert_eq!(cli.out, None);
        assert_eq!(cli.measured_vcpus, 0);
        assert_eq!(cli.measured_memory_bytes, 0);
    }

    #[test]
    fn every_plan_flag_is_accepted_and_lands_on_the_plan() {
        let cli = parse(&args(&[
            "--warmup",
            "1",
            "--rounds",
            "2",
            "--per-round",
            "3",
            "--concurrency",
            "4",
            "--fake-ms",
            "5",
        ]))
        .expect("应可解析");
        assert_eq!(cli.plan.warmup, 1);
        assert_eq!(cli.plan.rounds, 2);
        assert_eq!(cli.plan.per_round, 3);
        assert_eq!(cli.plan.concurrency, 4);
        assert_eq!(cli.plan.fake_command, Duration::from_millis(5));
    }

    /// 缩小计划能跑，但必须被判成非规格——这是 `conforms_to_spec` 存在的唯一理由。
    #[test]
    fn a_smaller_plan_is_parsed_but_not_spec() {
        let cli = parse(&args(&["--rounds", "1", "--per-round", "10"])).expect("应可解析");
        assert_eq!(cli.plan.rounds, 1);
        assert!(!cli.plan.is_spec_plan());
    }

    /// 退出码方向是 CI 的唯一输入，反了就是「跑通了报失败」。
    /// 三个方向都要钉死：跑通 0、越线 1、不可比 3。
    // `run_bench_on` 自己建运行时并 `block_on`，在 `#[tokio::test]` 里调用会 panic
    // （不能从运行时内再起运行时），所以这条必须是同步的 `#[test]`。
    #[test]
    fn the_exit_code_is_not_inverted() {
        use crate::outcome::Percentiles;

        let run = crate::runner::run_bench_on(quick(2, 8), 4, 8 * 1024 * 1024 * 1024)
            .expect("基准应可运行");

        // 方向一：规格计划 + 门禁成立 → 0。
        let passing = BenchRun {
            conforms_to_spec: true,
            ..run.clone()
        };
        assert!(
            passing.verdict().gate_passed,
            "小计划在 debug 下也该逐轮过 10 ms"
        );
        assert_eq!(exit_code(&passing), 0, "跑通必须是退出码 0");

        // 方向二：把每条样本的登记段改成 20 毫秒（> 10 毫秒门禁），逐轮 p95 必然越线。
        // 样本一个不删、不加——§11.3 不允许靠筛样本过门禁。
        let mut failing = BenchRun {
            conforms_to_spec: true,
            ..run
        };
        for round in &mut failing.rounds {
            for sample in &mut round.samples {
                sample.registration_nanos = 20_000_000;
                sample.gateway_nanos = 0;
            }
            let mut totals: Vec<u64> = round
                .samples
                .iter()
                .map(crate::outcome::total_nanos)
                .collect();
            totals.sort_unstable();
            round.percentiles = Percentiles::of(&totals);
        }
        assert!(!failing.verdict().gate_passed, "越线必须判不成立");
        assert_eq!(exit_code(&failing), 1, "门禁不成立必须报 1");

        // 方向三：计划不可比时固定 3，排在门禁之前——缩小计划不是失败，是不可比。
        let incomparable = BenchRun {
            conforms_to_spec: false,
            ..passing
        };
        assert_eq!(exit_code(&incomparable), 3, "不可比的计划必须报 3");
    }

    #[test]
    fn the_environment_is_explicit_never_guessed() {
        let cli = parse(&args(&["--vcpus", "8", "--mem-bytes", "17179869184"])).expect("应可解析");
        assert_eq!(cli.measured_vcpus, 8);
        assert_eq!(cli.measured_memory_bytes, 17_179_869_184);
    }

    #[test]
    fn the_output_directory_can_be_overridden() {
        let cli = parse(&args(&["--out", "/tmp/whatever"])).expect("应可解析");
        assert_eq!(cli.out, Some(PathBuf::from("/tmp/whatever")));
    }

    #[test]
    fn a_missing_value_is_an_error_not_a_silent_default() {
        assert!(matches!(
            parse(&args(&["--rounds"])),
            Err(CliError::Missing(flag)) if flag == "--rounds"
        ));
    }

    #[test]
    fn a_trailing_bare_value_is_an_error_not_a_silent_drop() {
        assert!(matches!(
            parse(&args(&["--rounds", "5", "--warmup"])),
            Err(CliError::Missing(_))
        ));
    }

    #[test]
    fn an_unknown_flag_is_rejected_instead_of_ignored() {
        assert!(matches!(
            parse(&args(&["--parallel", "8"])),
            Err(CliError::Unknown(flag)) if flag == "--parallel"
        ));
    }

    #[test]
    fn a_non_numeric_value_names_the_offending_flag_and_value() {
        match parse(&args(&["--rounds", "five"])) {
            Err(CliError::Invalid { flag, value }) => {
                assert_eq!(flag, "--rounds");
                assert_eq!(value, "five");
            }
            other => panic!("应报取值非法，实际 {other:?}"),
        }
    }

    /// `None` 必须渲染成 `-`：把「没测到」打印成 0 毫秒就是谎报。
    #[test]
    fn missing_milliseconds_render_as_a_dash_never_as_zero() {
        assert_eq!(ms(None), "-");
        assert_eq!(ms(Some(0)), "0.000 ms");
        assert_eq!(ms(Some(10_000_000)), "10.000 ms");
    }

    /// 逐轮口径必须把 §11.3 点名的字段全打出来。
    #[test]
    fn the_per_round_line_carries_every_section_11_3_field() {
        let usage = USAGE;
        assert!(usage.contains("cm60-bench"));
        let run = runner::run_bench(quick(1, 4)).expect("最小计划应可跑");
        let round = run.rounds.first().expect("应有一轮");
        assert_eq!(round.n(), 4);
        assert!(round.percentiles.p50_nanos.is_some());
        assert!(round.percentiles.p90_nanos.is_some());
        assert!(round.percentiles.p95_nanos.is_some());
        assert!(round.percentiles.p99_nanos.is_some());
        assert!(round.percentiles.max_nanos.is_some());
        assert_eq!(round.failures(), 0);
        assert_eq!(round.outcome.queued, 0);
    }

    /// 退出码的形状：规格运行 0/1，非规格运行 3，错误 2。三者不能混。
    #[test]
    fn a_non_spec_run_is_reported_as_incomparable_not_as_a_pass() {
        let run = runner::run_bench(quick(1, 4)).expect("最小计划应可跑");
        assert!(!run.conforms_to_spec);
        assert!(!run.comparable_to_criterion());
        let conclusions = run.verdict().conclusions;
        assert!(
            conclusions
                .iter()
                .any(|line| line.contains("不是 §11.1 的规格计划")),
            "非规格运行必须自己说不能下达标结论：{conclusions:?}"
        );
    }

    #[test]
    fn a_spec_plan_run_is_marked_as_comparable() {
        // 只验计划判定，不跑 5×10000——那是 release 基准的事，不该出现在单测里。
        assert!(DEFAULT_PLAN.is_spec_plan());
    }
}
