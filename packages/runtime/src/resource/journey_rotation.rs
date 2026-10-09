//! 轮换、配置版本与禁用旅程。
//!
//! 与 [`journey_ledger`](super::journey_ledger) 分开是因为这一组旅程关心的是**跨代**行为：
//! 上一代签发过的空闲是否还会被发出、上一代迟到的结果能不能回填新一代，
//! 以及连接被停用时在跑的执行按**真实**处置记账。
//!
//! 场景台架 [`Stage`] 定义在 [`harness`](super::harness) 里，两个旅程组共用。
//!
//! 时间只由 [`TestClock`](super::harness::TestClock) 显式推进，全程不 `sleep`。

use crate::connection::port::CancelDisposition;
use crate::connection::{ConfigRevision, ExecutionId, LeaseId};
use crate::resource::cleanup::{CancellationOutcome, DriverCleanVerdict, HostConditionSnapshot};
use crate::resource::generation::{CacheFillOutcome, RotationKind};
use crate::resource::harness::{Fault, Stage};
use crate::resource::lease::LeaseState;
// ---------------------------------------------------------------------------
// 池键轮换与缓存代
// ---------------------------------------------------------------------------

#[test]
fn a_credential_rotation_retires_the_idle_leases_of_the_previous_generation() {
    let mut stage = Stage::new();
    let lease = stage.acquire();
    let lease_id = lease.lease_id.clone();
    stage
        .manager
        .release(
            &lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("clean return to the idle pool");
    assert_eq!(stage.manager.idle_lease_count(), 1);

    let rotated_request = stage.request().at_credential_revision(1);
    let report = stage
        .manager
        .rotate(RotationKind::Credentials, &rotated_request)
        .expect("credential rotation");
    assert_eq!(report.retired_idles, vec![lease_id.clone()]);
    assert!(report.quarantined.is_empty());
    assert_eq!(stage.manager.idle_lease_count(), 0);
    assert!(stage.manager.lease(&lease_id).is_none());
    assert_eq!(
        stage.transport.snapshot().closed,
        vec![lease.resource_id.clone()],
        "旧代空闲必须真正关闭，而不只是从空闲表里移除"
    );

    // 新材料只能签发新代资源。
    let fresh = stage.acquire();
    assert_ne!(fresh.lease_id, lease_id);
    assert_eq!(fresh.pool_key.credential_revision, 1);
    assert_eq!(stage.transport.snapshot().opened.len(), 2);
}

#[test]
fn a_slow_cache_result_from_before_the_rotation_cannot_backfill_the_new_generation() {
    let mut stage = Stage::new();
    let request = stage.request();
    let slow_result = stage
        .manager
        .capture_cache_revision(&request)
        .expect("first capture registers the generation");
    let offered = slow_result.generation;

    stage
        .manager
        .rotate(
            RotationKind::NetworkRoute,
            &request.clone().at_network_route_revision(1),
        )
        .expect("route rotation");

    assert_eq!(
        stage
            .manager
            .admit_cache_fill(&stage.connection, &slow_result),
        CacheFillOutcome::RejectedStale {
            current: offered + 1,
            offered
        },
        "上一代迟到的结果不得回填新一代缓存"
    );

    let fresh_capture = stage
        .manager
        .capture_cache_revision(&request)
        .expect("capture under the new generation");
    assert_eq!(fresh_capture.generation, offered + 1);
    assert_eq!(
        stage
            .manager
            .admit_cache_fill(&stage.connection, &fresh_capture),
        CacheFillOutcome::Stored
    );
}

// ---------------------------------------------------------------------------
// 配置版本
// ---------------------------------------------------------------------------

#[test]
fn a_stale_expected_config_revision_is_refused_and_a_live_session_is_never_reconfigured() {
    let mut stage = Stage::new();
    let request = stage.request();
    stage
        .manager
        .capture_cache_revision(&request)
        .expect("register the connection at revision 1");

    let stale = stage
        .manager
        .acquire(
            &stage
                .request()
                .expecting_config_revision(ConfigRevision::new(7)),
        )
        .expect_err("过期 expectedRevision 必须按冲突拒绝");
    assert_eq!(
        stale,
        crate::resource::ResourceError::ConfigRevisionConflict {
            expected: 7,
            actual: 1
        }
    );

    let lease = stage
        .manager
        .acquire(
            &stage
                .request()
                .expecting_config_revision(ConfigRevision::new(1)),
        )
        .expect("对得上版本的请求正常签发");
    let lease_id = lease.lease_id.clone();
    assert_eq!(lease.config_revision, ConfigRevision::new(1));

    // ACL 轮换把宿主版本推到 2；旧会话**不被静默重配**，而是显式暴露漂移。
    stage
        .manager
        .rotate(RotationKind::AccessControl, &request)
        .expect("access control rotation");
    assert_eq!(
        stage
            .manager
            .lease(&lease_id)
            .expect("in-use lease survives")
            .config_revision,
        ConfigRevision::new(1),
        "已签发会话的配置版本不得被悄悄改写"
    );
    let drift = stage
        .manager
        .config_drift(&lease_id)
        .expect("版本漂移必须被报告出来");
    assert_eq!(drift.lease_revision, ConfigRevision::new(1));
    assert_eq!(drift.current_revision, ConfigRevision::new(2));
}

// ---------------------------------------------------------------------------
// 禁用 / 删除
// ---------------------------------------------------------------------------

/// 布置一条「正在执行」的租约，返回租约 id。
fn stage_with_running_execution(stage: &mut Stage) -> LeaseId {
    let lease = stage.acquire();
    let lease_id = lease.lease_id.clone();
    stage
        .manager
        .begin_execution(&lease_id, ExecutionId::new("exec-1"))
        .expect("the lease must accept a new execution");
    lease_id
}

#[test]
fn disabling_a_connection_drops_queued_requests_and_records_the_actual_cancellation() {
    let mut stage = Stage::new();
    let lease_id = stage_with_running_execution(&mut stage);
    stage
        .manager
        .enqueue(stage.request())
        .expect("queue one request while the connection is still enabled");

    let outcome = stage
        .manager
        .disable(&stage.connection)
        .expect("disable must be recorded, not silently applied");
    assert_eq!(outcome.dropped_requests, 1);
    assert!(outcome.closed_idle.is_empty(), "在执行的租约不是空闲租约");
    assert_eq!(outcome.cancellations.len(), 1);
    assert_eq!(
        outcome.cancellations[0].execution_id,
        ExecutionId::new("exec-1")
    );
    assert!(
        outcome.cancellations[0].outcome.cancelled(),
        "驱动接受了精确取消 ⇒ 如实记为 Cancelled"
    );
    assert!(outcome.quarantined.is_empty());
    assert_eq!(stage.manager.queued_len(), 0);

    // 禁用之后：新请求与排队请求都不执行。
    assert_eq!(
        stage
            .manager
            .acquire(&stage.request())
            .expect_err("禁用后不得再签发")
            .reason(),
        "resourceOwnerDisabled"
    );
    assert!(stage.manager.enqueue(stage.request()).is_err());
    assert_eq!(
        stage.transport.snapshot().opened.len(),
        1,
        "禁用后不得再开新的物理资源"
    );
    assert_eq!(
        stage.manager.lease(&lease_id).map(|lease| lease.state),
        Some(LeaseState::InUse)
    );
}

#[test]
fn an_unsupported_cancellation_is_recorded_as_unsupported_not_as_cancelled() {
    let mut stage = Stage::new();
    let lease_id = stage_with_running_execution(&mut stage);
    stage
        .transport
        .cancel_disposition(CancelDisposition::Unsupported);

    let outcome = stage
        .manager
        .disable(&stage.connection)
        .expect("disable records the disposition the driver actually returned");
    assert_eq!(outcome.cancellations.len(), 1);
    assert!(
        matches!(
            outcome.cancellations[0].outcome,
            CancellationOutcome::Unsupported { .. }
        ),
        "驱动不支持精确取消是正常返回值，不得谎报成已取消"
    );
    assert!(
        !outcome.cancellations[0].outcome.cancelled(),
        "Unsupported 不是 Cancelled"
    );
    assert!(
        outcome.quarantined.is_empty(),
        "不支持取消不等于取消失败，租约不该因此被隔离"
    );
    assert_eq!(
        stage.manager.lease(&lease_id).map(|lease| lease.state),
        Some(LeaseState::InUse)
    );
}

#[test]
fn a_failed_cancellation_quarantines_the_lease_instead_of_pretending_it_stopped() {
    let mut stage = Stage::new();
    let lease_id = stage_with_running_execution(&mut stage);
    stage.transport.inject(Fault::Cancel);

    let outcome = stage
        .manager
        .disable(&stage.connection)
        .expect("a failed cancellation is still a recorded outcome");
    assert!(
        matches!(
            outcome.cancellations[0].outcome,
            CancellationOutcome::Failed { .. }
        ),
        "取消失败必须按失败记账"
    );
    assert_eq!(outcome.quarantined, vec![lease_id.clone()]);
    assert_eq!(
        stage.manager.lease(&lease_id).map(|lease| lease.state),
        Some(LeaseState::Quarantined),
        "取消失败 ⇒ 物理资源进隔离，不得留在在执行态"
    );
}
