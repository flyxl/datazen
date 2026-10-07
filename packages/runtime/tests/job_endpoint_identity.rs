//! P5 端点身份集成测试（CM-31）：端点重叠检测只按**物理位置键** `service_key`
//! 判定——不同物理端点放行、同一物理端点自覆盖拒绝、身份不可证明时 fail-closed。
//!
//! `connection_id` 不是重叠键：它是 §6.2 记账键（`ensure_service` 按它注册），
//! 一条保存连接可以按库覆盖成多个会话。见本文件 `cm31_*` 两个跨库用例。

mod support;

use datazen_platform_api::id::ConnectionId;

use datazen_runtime::job::{detect_endpoint_overlap, EndpointRef, EndpointRole};

use support::conn;

/// 同名对象落在**两个不同**的物理端点上必须放行——这是数据迁移最常见的一步
/// （A 库的 public.users 拷到 B 库的 public.users）。
#[test]
fn cm31_same_object_on_two_different_endpoints_is_allowed() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_ok(),
        "不同物理端点之间拷贝同名对象必须允许"
    );
}

/// 两个不同的连接配置指向同一台服务器（别名）时仍必须拒绝：service_key 是共享的，
/// 而按配置 id 分桶看不见自覆盖。
#[test]
fn cm31_alias_configs_sharing_one_service_are_still_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("别名自覆盖必须拒绝");
    let datazen_runtime::job::JobError::EndpointOverlap(message) = err else {
        panic!("应当是 EndpointOverlap");
    };
    // 错误必须点名两端与对象，否则用户无从判断该改哪一侧
    assert!(message.contains("`users`"), "{message}");
    assert!(message.contains(conn().as_str()), "{message}");
    assert!(message.contains("conn-2"), "{message}");
}

/// 同名对象落在**同一台服务器的不同库**上必须放行。
///
/// 这是本轨 D1 缺陷的反向断言：`connect_dedicated` 只覆盖 `effective_config.database`，
/// `effective_config.id` 仍是同一条持久化连接 id（见
/// `services/connection_manager/connections.rs`），因此「同一 connectionId」不等于
/// 「同一物理端点」。把它当重叠键会让合法的跨库搬运被一句事实错误的
/// EndpointOverlap 拒绝——这类键只能把 accept 变成 reject，永远不能把 reject 变回
/// accept，即对真实自覆盖零增益。
#[test]
fn cm31_one_connection_id_across_two_databases_is_allowed() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-staging".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-prod".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_ok(),
        "同一条连接配置下的两个不同数据库是不同物理端点，必须允许"
    );
}

/// 同一 connectionId + **同一** service_key 的读写同名对象仍必须拒绝。
///
/// 与上一个用例成对：删掉 `connection_id` 这个重叠键，不能顺带把真正的自覆盖放行。
#[test]
fn cm31_same_service_key_on_one_connection_is_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("同库自覆盖必须拒绝");
    assert!(
        matches!(err, datazen_runtime::job::JobError::EndpointOverlap(_)),
        "{err:?}"
    );
}

/// fail-closed：端点给不出 `service_key`（身份不可证明）且同时存在 reader 与 writer
/// 时拒绝，不开放危险自覆盖。`connection_id` 非空**不再**能救它。
#[test]
fn cm31_unprovable_identity_with_writer_is_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "   ".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("身份不可证明必须拒绝");
    let datazen_runtime::job::JobError::EndpointOverlap(message) = err else {
        panic!("应当是 EndpointOverlap");
    };
    assert!(
        message.contains("unprovable"),
        "必须是身份不可证明这一条，而不是伪装成某个对象的自覆盖: {message}"
    );
}
