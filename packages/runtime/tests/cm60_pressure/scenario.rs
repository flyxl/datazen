//! CM-60 的压力场景：1000 次提交与取消、两个 worker、中途 drain 掉一个、最后关池。
//!
//! 全部时间走 `now_ms`——账本每个入口都显式收这个参数，所以这条 1000 步的压力链
//! **没有一次真实等待**，也没有定时器竞态：换台机器、换个负载，步序完全一致。
//!
//! 前置按判据逐字对齐：总额度 20、Control 保留 2（`[2,2,1,0]` ⇒ 共享池 15）、
//! 单用户逻辑 session 100、队列上限 32、短请求 fake 固定 10 ms。

use std::collections::BTreeMap;

use datazen_platform_api::id::{ConnectionId, DbSessionId, OrganizationId, PrincipalId, WorkerId};
use datazen_platform_api::ports::budget::ResourceClass;

use datazen_runtime::budget::queues::WaiterId;
use datazen_runtime::budget::records::SessionScope;
use datazen_runtime::budget::{
    AdmitOutcome, BudgetLedger, DenialReason, DrainScope, PermitRecord, ReleaseResult,
};

use crate::support::{claim, config, conn, org, DEADLINE_MS};

use super::journal::{Change, ResourceJournal, Watermark};

/// 判据总额度。
pub const TOTAL: u32 = 20;
/// 判据保留额：`Control = 2`，`Metadata = 1`，`Job = 0` ⇒ 共享池 `20 − 5 = 15`。
pub const RESERVED: [u32; 4] = [2, 2, 1, 0];
/// 单用户逻辑 session 上限。
pub const SESSIONS_PER_USER: u32 = 100;
/// 短请求 fake 的固定耗时。
pub const REQUEST_MS: u64 = 10;
/// 提交的操作数。
pub const OPERATIONS: usize = 1_000;

/// 参与分配额度的 worker 数。
const WORKERS: usize = 2;
/// 判据里的「100 用户」——每个用户每个节拍发一次短请求。
pub const USERS: usize = 100;
/// 节拍数：`100 用户 × 10 拍 = 1000 次操作`。一拍就是一个 `REQUEST_MS` 的 fake 耗时，
/// 所以一个请求正好占住一份 permit 到下一拍为止。
pub const TICKS: usize = OPERATIONS / USERS;
/// 每拍作废的等待句柄数（「提交 1000 次操作**与取消**」）。
const CANCELS_PER_TICK: usize = 8;
/// drain 掉其中一个 worker 的节拍（10 拍的中段）与拍内位置。
///
/// 取拍内中段而不是拍首：拍首时上一拍的短请求刚全部到期归还，池子是空的，
/// 被排空的节点手上必然没有 permit，「drain 不抢占」那条断言就空跑了。
const DRAIN_TICK: usize = TICKS / 2;
const DRAIN_AT_USER: usize = USERS / 2;
/// 操作流量用的 principal 数——与 session 持有者分开，让「每类每服务队列上限 32」
/// 成为先撞到的上界，而不是单用户队列上限。
const PRINCIPAL_COUNT: usize = USERS;

/// 一次提交的归宿。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SubmitOutcome {
    /// 直批放行。
    Granted,
    /// 入队等待。
    Queued,
    /// 终态拒绝。
    Rejected,
}

/// drain 之后单个节点上的提交账。
#[derive(Default, Clone, Copy, Debug)]
pub struct NodeCounters {
    /// 提交数。
    pub submits: u64,
    /// 直批放行数。
    pub granted: u64,
    /// 入队数。
    pub queued: u64,
    /// 终态拒绝数。
    pub rejected: u64,
    /// 其中因节点失联（drain）被拒的数。
    pub node_lost: u64,
}

impl NodeCounters {
    fn record(&mut self, outcome: SubmitOutcome, node_lost: bool) {
        match outcome {
            SubmitOutcome::Granted => self.granted += 1,
            SubmitOutcome::Queued => self.queued += 1,
            SubmitOutcome::Rejected => self.rejected += 1,
        }
        if node_lost {
            self.node_lost += 1;
        }
    }

    /// 被接纳的提交数（直批 + 入队）。
    pub fn accepted(&self) -> u64 {
        self.granted + self.queued
    }
}

/// 一整轮跑完的账。
#[derive(Debug)]
pub struct RunReport {
    /// 提交总数。
    pub submitted: usize,
    /// 放行总数（直批 + 队列放行）。
    pub admitted: u64,
    /// 归还的 permit 数。
    pub released: u64,
    /// 取消的等待者数（取消句柄作废）。
    pub cancelled: u64,
    /// 被队列上限拒掉的次数。
    pub queue_full: u64,
    /// 按 `DenialReason` 聚合的拒绝计数。
    pub denials: BTreeMap<String, u64>,
    /// drain 之后投给被排空 worker 的提交账。
    pub drained: NodeCounters,
    /// drain 之后投给另一个 worker 的提交账。
    pub peer: NodeCounters,
    /// drain 时被排空节点上的在手 permit 数（不抢占的证据）。
    pub live_on_drained_at_drain: usize,
    /// drain 报告里的未钉住数。
    pub drain_outstanding: usize,
    /// drain 报告里的钉住数。
    pub drain_pinned: usize,
    /// 开出的逻辑 session 数。
    pub sessions_opened: u32,
    /// 关池后残留的 permit 台账条数——必须为 0。
    pub leftover_permits: usize,
    /// 关池后能重新开出的逻辑 session 数——必须等于 `sessions_opened`：
    /// 开不出来就说明有 session 额度没随关池归还。
    pub sessions_reclaimable: u32,
    /// 虚跑时间总长。
    pub elapsed_ms: u64,
    /// 收尾时账本累计：放行 / 消费 / 归还。
    pub granted_total: u64,
    pub consumed_total: u64,
    pub returned_total: u64,
    /// 变化点台账。
    pub journal: ResourceJournal,
}

impl RunReport {
    /// 收尾水位。
    pub fn final_watermark(&self) -> Watermark {
        self.journal.final_watermark()
    }
}

/// 跑完一整轮 CM-60 场景。
pub fn run() -> RunReport {
    let budget_config = config(TOTAL, RESERVED);
    let service_quota = budget_config.service_quota;
    let queue_cap = budget_config.queue_cap_per_class_per_service;
    let journal = ResourceJournal::new(service_quota, queue_cap);

    let organization: OrganizationId = org();
    let connection: ConnectionId = conn();
    let workers: Vec<WorkerId> = (0..WORKERS)
        .map(|index| WorkerId::new(format!("worker-{index}")))
        .collect();
    let principals: Vec<PrincipalId> = (0..PRINCIPAL_COUNT)
        .map(|index| PrincipalId::new(format!("u-{index}")))
        .collect();
    // 逻辑 session 归一个人——判据要的就是「单用户 100」这条线。
    let session_owner = PrincipalId::new("u-session-owner");

    let mut ledger = BudgetLedger::new(budget_config);
    ledger.ensure_service(&connection);

    let mut now_ms = 0u64;
    // 在手 permit 连同它的到期时刻：短请求 fake 固定耗时 `REQUEST_MS`，所以每个请求
    // 在下一拍到期。带上到期时刻，释放就由虚拟时间决定，而不是由「挑哪一个释放」决定。
    let mut live: Vec<(PermitRecord, u64)> = Vec::new();
    let mut waiting: Vec<WaiterId> = Vec::new();
    let mut admitted = 0u64;
    let mut released = 0u64;
    let mut cancelled = 0u64;
    let mut queue_full = 0u64;
    let mut denials: BTreeMap<String, u64> = BTreeMap::new();
    let mut drained_counters = NodeCounters::default();
    let mut peer_counters = NodeCounters::default();
    let mut live_on_drained_at_drain = 0usize;
    let mut drain_outstanding = 0usize;
    let mut drain_pinned = 0usize;
    let mut drain_index = 0usize;
    let mut drained = false;

    // ── 前置：100 个逻辑 session ────────────────────────────────────────────
    let mut tickets: Vec<DbSessionId> = Vec::with_capacity(SESSIONS_PER_USER as usize);
    for _ in 0..SESSIONS_PER_USER {
        match ledger.open_session(&organization, &session_owner, &connection) {
            Ok(ticket) => {
                tickets.push(ticket);
                journal.observe(&ledger, &connection, now_ms, Change::SessionOpened);
            }
            Err(reason) => panic!(
                "CM-60 前置：第 {} 个逻辑 session 应当放行，却被拒：{reason:?}",
                tickets.len() + 1
            ),
        }
    }
    let sessions_opened = tickets.len() as u32;
    // 第 101 个必须按「单用户 100」被拒——这条上界得是活的，不是恰好没人碰。
    let session_cap_rejected = match ledger.open_session(&organization, &session_owner, &connection)
    {
        Ok(ticket) => panic!("CM-60 前置：第 101 个逻辑 session 不该放行，却拿到 {ticket:?}"),
        Err(reason) => {
            assert_eq!(
                reason,
                DenialReason::SessionQuotaExceeded {
                    scope: SessionScope::PerUser,
                    cap: SESSIONS_PER_USER,
                    used: SESSIONS_PER_USER,
                },
                "CM-60 前置：越界 session 应按单用户 100 被拒",
            );
            1
        }
    };
    assert_eq!(session_cap_rejected, 1);

    // ── 步骤：1000 次提交与取消；两个 worker 分配额度；中途 drain 一个 ────────
    //
    // 模型逐字对齐判据前置：100 个用户，每个用户每个节拍发一次 fake 固定耗时
    // `REQUEST_MS` 的短请求，所以 10 拍 × 100 = 1000 次操作，一份 permit 正好被
    // 占住一个节拍。每拍 100 并发压 20 份额度，排队上限与终态拒绝自然被压出来。
    for tick in 0..TICKS {
        // ① 短请求 fake 的固定耗时：虚拟时间只由这一拍推进。
        now_ms += REQUEST_MS;

        // ② 取消（判据里的「1000 次操作与取消」）。
        //
        // 排空节点的那一拍先**把积压清空**。这不是为了迁就断言：队列放行的 permit
        // 按设计不带 worker 绑定（`pump` 抹掉 `worker`），所以只要积压是满的，池子
        // 每拍都被 `pump` 补满，新到的请求永远走不到直批，两个节点手上的 permit 数
        // 就恒为 0——此时 drain 必然挑到空手节点，「drain 不抢占」只能空跑。
        // 先清积压再排空一个 worker，也是运维上的真实次序。
        let cancel_budget = if tick == DRAIN_TICK {
            usize::MAX
        } else {
            CANCELS_PER_TICK
        };
        for _ in 0..cancel_budget {
            if waiting.is_empty() {
                break;
            }
            let waiter = waiting.remove(0);
            if ledger.cancel(waiter) {
                cancelled += 1;
                journal.observe(&ledger, &connection, now_ms, Change::WaiterCancelled);
            }
        }

        // ③ 本拍到期的短请求完成，归还额度后推进队列。
        let mut due = Vec::new();
        let mut still_running = Vec::new();
        for (record, deadline) in live.drain(..) {
            if deadline <= now_ms {
                due.push(record);
            } else {
                still_running.push((record, deadline));
            }
        }
        live = still_running;
        for done in due {
            match ledger.release(&done.to_port_permit(), true, now_ms) {
                ReleaseResult::Released { record } => {
                    released += 1;
                    journal.observe(
                        &ledger,
                        &connection,
                        now_ms,
                        Change::Released {
                            class: record.class,
                        },
                    );
                }
                other => panic!("CM-60：到期归还在手 permit 应当成功，却得到 {other:?}"),
            }
        }
        let pumped = ledger.pump(now_ms);
        assert!(
            pumped.timed_out.is_empty(),
            "CM-60：虚拟时间只有 {} ms，短于等待期限，不该出现超时离队",
            now_ms,
        );
        for record in pumped.granted {
            let class = record.class;
            live.push((record, now_ms + REQUEST_MS));
            admitted += 1;
            journal.observe(&ledger, &connection, now_ms, Change::Admitted { class });
        }

        // ④ 提交本拍 100 份请求：每拍 100 并发压 20 份额度，排队上限与拒绝自然被压出来。
        for user in 0..USERS {
            let index = tick * USERS + user;
            let worker = &workers[(tick + user) % WORKERS];
            let class = traffic_class(index);
            let principal = principals[user % PRINCIPAL_COUNT].clone();
            let mut request = claim(&organization, &connection, principal.as_str(), class)
                .on_worker(worker.clone());
            // 每 5 份里有 1 份是「已建立连接 / 活跃事务 / 游标」类：钉住，drain 不得抢占。
            if index % 5 == 0 {
                request = request.pinned();
            }
            let on_drained_node = drained && *worker == workers[drain_index];

            // 归宿先算出来，再入台账——drain 之后两个节点要分开记账。
            let mut outcome = SubmitOutcome::Rejected;
            let mut node_lost = false;
            match ledger.try_admit(&request, now_ms) {
                AdmitOutcome::Granted(record) => {
                    live.push((record, now_ms + REQUEST_MS));
                    admitted += 1;
                    outcome = SubmitOutcome::Granted;
                    journal.observe(&ledger, &connection, now_ms, Change::Admitted { class });
                }
                AdmitOutcome::Busy(reason) if is_waitable(&reason) => {
                    // 额度暂时不够 → 排队（`coordinator` 的路径：先试批，再入队）。
                    match ledger.enqueue(&request, now_ms, now_ms + DEADLINE_MS) {
                        Ok(waiter) => {
                            waiting.push(waiter);
                            outcome = SubmitOutcome::Queued;
                            journal.observe(&ledger, &connection, now_ms, Change::Queued { class });
                        }
                        Err(DenialReason::QueueFull { .. }) => {
                            queue_full += 1;
                            journal.observe(
                                &ledger,
                                &connection,
                                now_ms,
                                Change::QueueRejected { class },
                            );
                        }
                        Err(reason) => {
                            node_lost = matches!(reason, DenialReason::NodeLost { .. });
                            note_denial(&mut denials, &reason);
                            journal.observe(&ledger, &connection, now_ms, Change::Denied);
                        }
                    }
                }
                AdmitOutcome::Busy(reason) | AdmitOutcome::Denied(reason) => {
                    node_lost = matches!(reason, DenialReason::NodeLost { .. });
                    note_denial(&mut denials, &reason);
                    journal.observe(&ledger, &connection, now_ms, Change::Denied);
                }
            }
            if drained {
                let counters = if on_drained_node {
                    &mut drained_counters
                } else {
                    &mut peer_counters
                };
                counters.submits += 1;
                counters.record(outcome, node_lost);
            }

            // 排空节点：drain 之后不再接新资源（§9.5）。
            //
            // 判据只说「将一个 worker drain」。挑**当场手上真有活**的那个，否则
            // 「drain 不抢占」这条断言会空跑——pump 放行的 permit 不带 worker 绑定，
            // 随手挑一个节点很可能正好挑空手节点。
            if tick == DRAIN_TICK && user == DRAIN_AT_USER {
                let held = |worker: &WorkerId| {
                    live.iter()
                        .filter(|(record, _)| record.worker.as_ref() == Some(worker))
                        .count()
                };
                let first = held(&workers[0]);
                let second = held(&workers[1]);
                drain_index = usize::from(second > first);
                let node = workers[drain_index].clone();
                let peer_node = workers[WORKERS - 1 - drain_index].clone();

                let before = held(&node);
                assert!(
                    before > 0,
                    "CM-60 断言四：被排空的节点手上没有在手 permit，「drain 不抢占」无从检验",
                );
                let report = ledger.drain(DrainScope::Node(node.clone()));
                let after = held(&node);
                assert!(report.draining, "CM-60 断言四：drain 后节点应进入排空态");
                assert_eq!(
                    before, after,
                    "CM-60 断言四：drain 不得抢占被排空节点上已到手的 permit（{before} → {after}）",
                );
                assert_eq!(
                before,
                report.outstanding + report.pinned,
                "CM-60 断言四：drain 报告应覆盖该节点全部在手 permit（{before} vs 报告 {} + {}）",
                report.outstanding,
                report.pinned,
            );
                assert!(
                    !ledger.node_is_draining(&peer_node),
                    "CM-60 断言四：drain 是节点级的，另一个节点不该被牵连",
                );
                live_on_drained_at_drain = after;
                drain_outstanding = report.outstanding;
                drain_pinned = report.pinned;
                drained = true;
                journal.observe(&ledger, &connection, now_ms, Change::NodeDrained);
            }
        }
    }

    // ── 步骤收尾：全部完成后关池 ───────────────────────────────────────────
    // 归还在手 permit。
    while let Some((record, _)) = live.pop() {
        match ledger.release(&record.to_port_permit(), true, now_ms) {
            ReleaseResult::Released { record } => {
                released += 1;
                journal.observe(
                    &ledger,
                    &connection,
                    now_ms,
                    Change::Released {
                        class: record.class,
                    },
                );
            }
            other => panic!("CM-60：关池时归还在手 permit 应当成功，却得到 {other:?}"),
        }
    }
    // 取消所有等待句柄。
    for waiter in std::mem::take(&mut waiting) {
        if ledger.cancel(waiter) {
            cancelled += 1;
            journal.observe(&ledger, &connection, now_ms, Change::WaiterCancelled);
        }
    }
    // 关掉全部逻辑 session。
    for ticket in &tickets {
        match ledger.close_session(ticket) {
            Ok(()) => journal.observe(&ledger, &connection, now_ms, Change::SessionClosed),
            Err(reason) => panic!("CM-60：关池时关闭逻辑 session 应当成功，却被拒：{reason:?}"),
        }
    }
    journal.observe(&ledger, &connection, now_ms, Change::PoolClosed);

    // 关池探针：额度应当全部归还。重新开出同样多的逻辑 session 必须全部成功——
    // 开不出来就说明有 session 额度没随关池释放。探针本身不入变化点台账，
    // 台账记录的是关池那一刻的终态。
    let mut sessions_reclaimable = 0;
    for _ in 0..SESSIONS_PER_USER {
        match ledger.open_session(&organization, &session_owner, &connection) {
            Ok(ticket) => {
                sessions_reclaimable += 1;
                match ledger.close_session(&ticket) {
                    Ok(()) => {}
                    Err(reason) => {
                        panic!("CM-60 关池探针：回收重开的 session 应当成功，却被拒：{reason:?}")
                    }
                }
            }
            Err(reason) => {
                panic!(
                    "CM-60 关池探针：第 {} 个 session 额度未随关池归还：{reason:?}",
                    sessions_reclaimable + 1
                )
            }
        }
    }

    assert!(
        ledger.node_is_draining(&workers[drain_index]),
        "CM-60 断言四：被排空的节点应保持排空态",
    );
    assert!(
        !ledger.node_is_draining(&workers[WORKERS - 1 - drain_index]),
        "CM-60 断言四：只排空一个节点",
    );

    RunReport {
        submitted: OPERATIONS,
        admitted,
        released,
        cancelled,
        queue_full,
        denials,
        drained: drained_counters,
        peer: peer_counters,
        live_on_drained_at_drain,
        drain_outstanding,
        drain_pinned,
        sessions_opened,
        leftover_permits: ledger.permits().count(),
        sessions_reclaimable,
        elapsed_ms: now_ms,
        granted_total: ledger.granted_total(),
        consumed_total: ledger.consumed_total(),
        returned_total: ledger.returned_total(),
        journal,
    }
}

/// 只有「现在没位子、以后可能有」的拒绝才值得排队；其余是终态，排队也没用。
fn is_waitable(reason: &DenialReason) -> bool {
    matches!(reason, DenialReason::Exhausted { .. })
}

fn note_denial(denials: &mut BTreeMap<String, u64>, reason: &DenialReason) {
    *denials.entry(format!("{reason:?}")).or_insert(0) += 1;
}

/// 操作流量的类别分布：`Interactive` 6 / `Metadata` 2 / `Job` 2。
///
/// **刻意不投 `Control`**——只有普通流量压上来，「普通资源不侵占预留」这条断言才有意义。
fn traffic_class(index: usize) -> ResourceClass {
    match index % 5 {
        0..=2 => ResourceClass::Interactive,
        3 => ResourceClass::Metadata,
        _ => ResourceClass::Job,
    }
}
