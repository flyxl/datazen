//! 候选替换的连续旅程测试。
//!
//! 与 [`journey`](super::journey) 分开是因为这一组旅程要驱动**目录端口**，
//! 断言的是「两份不同的最终状态」，而不是单点裁决：
//!
//! * 提交前失败 ⇒ 旧会话仍然有效、候选被清理；
//! * 提交成功但目录写失败 ⇒ **绝不**报成功，且只能取回**同一份**回执。
//!
//! 两者的终态必须不同，否则「不许成功但无记录」这条约束就是空的。
//! 跨代旅程在 [`journey_rotation`](super::journey_rotation)。
//!
//! 场景台架 [`Stage`](super::harness::Stage) 与时钟替身共用，旅程里**从不** `sleep`。

use crate::connection::{DbSessionId, ResourceId};
use crate::resource::harness::{Stage, TransportEvent};
use crate::resource::publication::DirectoryFault;
use crate::resource::replacement::{BeginOutcome, CandidateState};

// ---------------------------------------------------------------------------
// 候选替换
// ---------------------------------------------------------------------------

#[test]
fn a_failure_before_commit_leaves_the_old_session_executable_and_cleans_the_candidate() {
    let mut stage = Stage::new();
    let old_session = DbSessionId::new("db-old-1");
    let new_session = DbSessionId::new("db-new-1");
    let candidate_lease = stage.open_candidate("idem-1", &old_session, &new_session);

    assert!(
        stage.manager.old_session_executable(&old_session),
        "候选还没提交，旧会话当然仍然可执行"
    );
    assert_eq!(
        stage
            .manager
            .accepts_execution(&candidate_lease)
            .expect_err("未发布的候选绝不能执行"),
        // 断言的是「拒绝」，而不是拒绝文案。
        crate::resource::ResourceError::CandidateNotPublished
    );

    // 提交**之前**失败：销毁候选，旧会话保留。
    stage
        .manager
        .abandon_replacement("idem-1")
        .expect("the candidate must be cleanable before commit");

    assert!(
        stage.manager.old_session_executable(&old_session),
        "提交前失败必须让旧会话继续有效"
    );
    assert!(
        stage.manager.lease(&candidate_lease).is_none(),
        "候选租约必须从宿主台账里消失"
    );
    assert!(
        stage.directory.visible_records() == 0,
        "提交前失败绝不能在公开目录里留下记录"
    );
    assert_eq!(
        stage.transport.snapshot().events,
        vec![
            TransportEvent::Open(ResourceId::new("res-1")),
            TransportEvent::Close(ResourceId::new("res-1")),
        ],
        "候选的物理连接必须被真正关闭，而不是仅仅从台账里抹掉"
    );
}

#[test]
fn a_commit_whose_directory_publish_fails_is_never_reported_as_success() {
    let mut stage = Stage::new();
    let old_session = DbSessionId::new("db-old-2");
    let new_session = DbSessionId::new("db-new-2");
    let candidate_lease = stage.open_candidate("idem-2", &old_session, &new_session);
    stage
        .directory
        .fail_with(DirectoryFault::Rejected("attachment token already bound"));

    let error = stage
        .manager
        .confirm_replacement("idem-2", stage.connection.clone(), "attach-2")
        .expect_err("提交成功但目录写失败绝不能报成功");
    assert_eq!(error.reason(), "resourceCandidateNotPublished");
    assert_eq!(
        stage.directory.rejected_tokens(),
        vec!["attach-2".to_string()],
        "目录确实拒收过这份回执，而不是我们单方面没写"
    );
    assert!(
        !stage.manager.old_session_executable(&old_session),
        "提交已经落成，旧 ID 不再执行"
    );
    assert!(
        !stage.directory.records_session(&new_session),
        "没有目录记录就不是替换成功"
    );
    assert!(
        stage.manager.accepts_execution(&candidate_lease).is_err(),
        "目录记录缺席期间，候选仍然不能对外服务"
    );

    // 目录恢复后重试：取回的是**同一份**回执，不是另算一份。
    stage.directory.recover();
    let receipt = stage
        .manager
        .recover_replacement("idem-2")
        .expect("the committed replacement must be recoverable");
    assert_eq!(receipt.new_session_id, new_session);
    assert_eq!(receipt.old_session_id, old_session);
    assert!(
        receipt.published_at_nanos.is_some(),
        "补齐目录记录之后才算发布完成"
    );
    assert_eq!(stage.directory.visible_records(), 1);
    assert!(
        stage.manager.accepts_execution(&candidate_lease).is_ok(),
        "发布之后候选才允许执行"
    );
}

#[test]
fn the_same_idempotency_key_resumes_the_candidate_instead_of_opening_a_second_resource() {
    let mut stage = Stage::new();
    let old_session = DbSessionId::new("db-old-3");
    let new_session = DbSessionId::new("db-new-3");
    stage.open_candidate("idem-3", &old_session, &new_session);

    let request = stage.request();
    let resumed = stage
        .manager
        .begin_replacement(
            "idem-3",
            old_session.clone(),
            DbSessionId::new("db-ignored"),
            ResourceId::new("res-old"),
            &request,
        )
        .expect("同一幂等键的重试必须续用既有候选");
    assert_eq!(
        resumed,
        BeginOutcome::Resumed {
            new_session_id: new_session,
            state: CandidateState::Proposed
        }
    );
    assert_eq!(
        stage.transport.snapshot().opened.len(),
        1,
        "重试绝不能再建一条物理连接"
    );

    let collision = stage
        .manager
        .begin_replacement(
            "idem-3",
            DbSessionId::new("db-other-old"),
            DbSessionId::new("db-other-new"),
            ResourceId::new("res-other"),
            &request,
        )
        .expect_err("同一幂等键指向另一个旧会话就是缺陷");
    assert_eq!(collision.reason(), "resourceDuplicateCandidate");
    assert_eq!(stage.transport.snapshot().opened.len(), 1);
}
