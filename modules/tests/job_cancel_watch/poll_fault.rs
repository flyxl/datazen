//! C3 补强：轮询读失败的观测窗口必须落在**阶段仍在执行**的时候。
//!
//! 上一个用例（`cancel_poll_failure_stops_polling_without_failing_the_stage`）在阶段
//! **返回之后**才采样读次数，那时看守者早已被 abort + await，等于什么都没量。
//! 本文件把观测窗口搬进阶段执行期，并独立承担告警捕获器，因此单独成文件以守住
//! 800 行规模。

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::{Event, Id, Metadata, Subscriber};

use super::*;

// ---------------------------------------------------------------- 告警捕获

/// 极简 `tracing::Subscriber`：只把 WARN 事件按 jobId 归档。
///
/// 不引 `tracing-subscriber`（那会改动 `Cargo.lock`，超出本轨的 inScope）：
/// `tracing` 本身已经是 runtime 的依赖，测试目标可以直接用 `Subscriber` trait 手写。
#[derive(Default)]
struct WarnRecorder {
    events: Mutex<Vec<(String, String)>>,
}

#[derive(Default)]
struct WarnFields {
    job_id: Option<String>,
    message: Option<String>,
}

impl Visit for WarnFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "job_id" => self.job_id = Some(value.to_string()),
            "message" => self.message = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}

impl Subscriber for WarnRecorder {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    // 本订阅者只关心事件，span 的生命周期与后续关系一律忽略。
    fn record(&self, _span: &Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        let mut fields = WarnFields::default();
        event.record(&mut fields);
        self.events.lock().expect("warn recorder").push((
            fields.job_id.unwrap_or_default(),
            fields.message.unwrap_or_default(),
        ));
    }
}

impl WarnRecorder {
    /// 某个 job 收到的 WARN 原文。
    fn warns_for(&self, job_id: &str) -> Vec<String> {
        self.events
            .lock()
            .expect("warn recorder")
            .iter()
            .filter(|(id, _)| id == job_id)
            .map(|(_, message)| message.clone())
            .collect()
    }
}

/// 进程内只能装一个全局订阅者，本测试二进制里没有别人装；按 jobId 过滤因此也不会
/// 被同进程内并行跑的其它用例污染。
fn install_warn_recorder() -> Arc<WarnRecorder> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let recorder = Arc::new(WarnRecorder::default());
    let installed = recorder.clone();
    ONCE.call_once(move || {
        let _ = tracing::subscriber::set_global_default(installed);
    });
    recorder
}

// ---------------------------------------------------------------- C3（补强）：故障窗口内就要量

/// C3 补强：把「读次数不再增长」的观测放在阶段执行期间：
///
/// 0. **先证明前提**：故障注入之前，看护者的读次数跨过 `QUIET_WINDOW` 必须真的在长。
///    没有这一步，下面第 2 步的「读次数零增长」是个空转断言——把 `repository.rs` 里
///    `cancel_poll` 的 `get_calls.fetch_add` 删掉，它照样通过（`before == after` 恒成立）。
/// 1. 阶段执行期间 handler（这里由测试侧）打开读故障，看守者撞上第一次失败 → 一条 WARN；
/// 2. 阶段继续卡着不动（测试还没放行），跨过 `QUIET_WINDOW` 再采样：
///    读次数**零增长**、WARN **仍然只有一条**——即「告警后停止轮询」是真的停，
///    而不是每 50ms 失败一次刷一条告警；
/// 3. 放行阶段 → 关故障 → 阶段正常 `Succeeded`（读失败不得 fail-closed）。
///
/// 删掉 `runtime.rs` 里 `tracing::warn!` 之后那个 `return;`，第 2 步的两条断言都会红；
/// 删掉 `repository.rs` 里 `cancel_poll` 的 `get_calls.fetch_add`，第 0 步就会红。
#[tokio::test]
async fn poll_failure_stops_polling_while_the_stage_is_still_running() {
    let job_id = "job-cw-8";
    let warns = install_warn_recorder();
    let mut rig = rig(job_id, vec![("s1", Plan::Held)]).await;
    let run = rig.start();

    match rig.next().await {
        Msg::Entered {
            stage,
            cancel_at_entry,
            watchers_at_entry,
            get_calls_at_entry,
        } => {
            assert_eq!(stage, "s1");
            assert!(!cancel_at_entry);
            assert_eq!(watchers_at_entry, 1, "阶段执行期间必须有一个看守者在飞");
            // 第 0 步：故障还没注入，先证明看护者真的在读。
            tokio::time::sleep(QUIET_WINDOW).await;
            assert!(
                rig.repo.get_calls() > get_calls_at_entry,
                "注入故障前看护者必须真的在轮询（跨过 {QUIET_WINDOW:?} 读次数必须增长），\
                 否则下面『停止轮询』测的是一个从不轮询的计数器"
            );
        }
        other => panic!("期望进入 s1，得到 {other:?}"),
    }

    // 现在才注入读故障：注入时刻由测试决定，观测对象也就由测试决定。
    rig.repo.fail_get(true);

    // 等看守者真的撞上故障：告警出现。不出现就说明这个用例连故障路径都没走到。
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while warns.warns_for(job_id).is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "阶段执行期间从未出现读失败告警，观测窗口没落在故障路径上"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // 关键：此刻阶段**仍在执行**（还没收到放行信号），所以这里量的是活跃轮询。
    let before = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    let after = rig.repo.get_calls();
    assert_eq!(
        after, before,
        "读失败后必须在阶段执行期间就停止轮询（跨过 {QUIET_WINDOW:?} 读次数一个字都不许长）"
    );
    assert_eq!(
        warns.warns_for(job_id).len(),
        1,
        "一个阶段的读失败只允许一条告警，得到 {:?}",
        warns.warns_for(job_id)
    );
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        1,
        "停止的是轮询，不是任务：看守者要活到阶段返回才由 dispatch 回收"
    );

    // 放行阶段：先关掉故障，否则 `dispatch` 自己的读也会失败。
    rig.repo.fail_get(false);
    rig.release.notify_one();
    match rig.next().await {
        Msg::Finished {
            observed_cancel, ..
        } => assert!(
            !observed_cancel,
            "读失败不得被当成取消：失败只能让我们错过取消，不能凭空造出取消"
        ),
        other => panic!("期望阶段结束事件，得到 {other:?}"),
    }
    let result = run.await.expect("join").expect("run");
    assert_eq!(
        result.state,
        JobState::Succeeded,
        "轮询失败按决策 (a) 处理：让阶段跑完，而不是 fail-closed"
    );
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(rig.runtime.active_cancel_watchers(), 0);

    let all = warns.warns_for(job_id);
    assert_eq!(
        all.len(),
        1,
        "整个阶段只允许一条告警：既不能被静默吞掉，也不能一轮一条地刷屏"
    );
    assert!(
        all[0].contains("cancel watch"),
        "告警必须说明发生了什么，得到 {:?}",
        all[0]
    );
}
