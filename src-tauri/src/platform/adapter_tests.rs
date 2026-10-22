//! `platform::adapter` 的结构性用例。
//!
//! 由 `adapter.rs` 以 `#[path]` 在 `#[cfg(test)]` 下装载：`super::` 因此仍指向 `adapter` 模块，
//! 且本文件与 `adapter.rs` 同目录，`include_str!("adapter.rs")` 的解析基准与拆出前逐字节一致。
use super::*;
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId, JobId, StageId};

fn adapter() -> PlatformAdapter {
    PlatformAdapter::with_identity(
        DesktopIdentity::new_launch("datazen-desktop-local", "alice", "client-1")
            .expect("固定输入应当组装成功"),
    )
}

#[test]
fn assembly_state_is_derived_from_injection_not_hand_kept() {
    let adapter = adapter();
    // 这条断言读的是 OnceLock 推导结果，不是常量，因此注入逻辑写错时会失败。
    assert_eq!(adapter.assembly_state(), AssemblyState::IdentityReady);
    let missing: Vec<&str> = adapter
        .assembly_state()
        .missing_steps()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        missing,
        vec!["§6.2-6 Runtime", "§6.2-7 ApplicationServices"],
        "缺哪两步必须可枚举，否则调用方无法定位"
    );
    // 每条缺步记录的目标状态必须比当前状态靠后，否则"补齐到哪"是错的。
    for (_, after) in AssemblyState::IdentityReady.missing_steps() {
        assert_ne!(*after, AssemblyState::IdentityReady);
    }
    assert!(AssemblyState::FullyWired.missing_steps().is_empty());
    // 组装状态与注入事实必须一致：此刻未注入，就取不到应用服务。
    assert!(adapter.application_services().is_none());
    assert_eq!(adapter.identity().principal_id().as_str(), "alice");
    assert!(adapter.handles().is_empty());
}

#[test]
fn launch_without_a_deterministic_desktop_user_fails_closed() {
    // `new_launch` 依赖运行环境变量；这里只断言它要么成功、要么明确失败，
    // 绝不返回占位主体。
    match PlatformAdapter::new_launch("client-x") {
        Ok(adapter) => assert!(!adapter.identity().principal_id().as_str().is_empty()),
        Err(err) => assert_eq!(err.code, ApiErrorCode::Unauthenticated),
    }
}

#[test]
fn failed_entry_returns_the_original_error_not_a_placeholder_adapter() {
    // 组装失败时，依赖身份的调用必须拿到**原始**原因。
    // 如果这里退化成一个占位 adapter，调用方会以为身份已就绪，
    // 然后把未知主体当成已知主体写进日志/审计 —— 那是 :84 禁止的伪映射。
    let original = ApiError::new(
        ApiErrorCode::Unauthenticated,
        "test: 桌面用户不可解析，身份未组装",
    );
    let entry = PlatformEntry::failed(original.clone());
    assert!(!entry.is_ready());
    let err = entry.require().expect_err("失败的 entry 必须报错");
    assert_eq!(err.code, ApiErrorCode::Unauthenticated);
    assert_eq!(err.message, original.message);
    // 失败时不得凭空造出一个 adapter。
    assert!(PlatformEntry::failed(original.clone()).require().is_err());
}

#[test]
fn ready_entry_hands_back_the_same_adapter() {
    let adapter = adapter();
    let entry = PlatformEntry::ready(adapter);
    assert!(entry.is_ready());
    let recovered = entry.require().expect("成功的 entry 必须给出 adapter");
    assert_eq!(recovered.identity().principal_id().as_str(), "alice");
}

#[test]
fn each_call_gets_a_fresh_request_id_and_a_stable_principal() {
    let adapter = adapter();
    let a = adapter.begin_call();
    let b = adapter.begin_call();
    assert_ne!(a.request_id(), b.request_id());
    assert_eq!(a.context().principal_id.as_str(), "alice");
    assert_eq!(b.context().principal_id.as_str(), "alice");
    assert_eq!(a.context().client_instance_id.as_str(), "client-1");
}

#[test]
fn owned_call_rejects_a_foreign_editor_owner_before_any_use_case_runs() {
    let adapter = adapter();
    let err = adapter
        .begin_owned_call(&OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-other"),
            editor_session_id: EditorSessionId::new("tab-1"),
        })
        .err()
        .expect("跨实例 editor owner 必须被拒");
    assert_eq!(err.code, ApiErrorCode::PermissionDenied);
}

#[test]
fn owned_call_accepts_the_current_instance_editor_owner() {
    let adapter = adapter();
    let (scope, owner) = adapter
        .begin_owned_call(&OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("tab-1"),
        })
        .expect("同实例 editor owner 应通过");
    assert!(matches!(
        owner,
        datazen_platform_api::context::OwnerRef::Editor { .. }
    ));
    assert_eq!(scope.context().client_instance_id.as_str(), "client-1");
}

#[test]
fn owned_call_fails_closed_for_job_owners() {
    let adapter = adapter();
    let err = adapter
        .begin_owned_call(&OwnerIntent::Job {
            job_id: JobId::new("job-1"),
            stage_id: StageId::new("stage-1"),
        })
        .err()
        .expect("job owner 必须失败关闭");
    assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
}

#[test]
fn application_services_is_absent_and_the_failure_explains_why() {
    let adapter = adapter();
    assert!(
        adapter.application_services().is_none(),
        "inject() 之前不得凭空造出 ApplicationServices"
    );
    let err = adapter
        .require_application_services()
        .err()
        .expect("未注入必须报错");
    assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
    // 报错必须说明未注入这件事，否则调用方无法定位是启动装配漏了哪一步。
    for needle in ["§6.2", "未注入", "inject", "不得改走旧共享 session"] {
        assert!(
            err.message.contains(needle),
            "报错缺少「{needle}」：{}",
            err.message
        );
    }
}

/// 窄适配的核心约束：**不得回落**。`require_*` 失败时不能偷偷给旧路径开门。
#[test]
fn failing_requirements_never_fall_back_to_the_legacy_path() {
    let adapter = adapter();
    // 三个 require_* 在未注入 / 未接线时的行为都必须是 Err，而不是"尽力而为"。
    assert!(adapter.require_application_services().is_err());
    // 桥在测试进程里可能被其它用例注入，两种结果都必须成功，
    // 关键是**没有第三条"回落"分支**。
    match adapter.require_bridge() {
        Ok(_) => {}
        Err(err) => assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported),
    }
}

/// 注入点常量必须指向真实符号，否则"可定位"的承诺是假的。
///
/// 这条断言随注入缝隙一起搬到了 `packages/application/src/frontend_bridge.rs`，
/// 但它校验的是**宿主侧**符号，所以留在 adapter 的用例里：常量在用例层，
/// 被指名的 `pub fn set_bridge_injection` 在组装层，两端必须同时可读。
#[test]
fn injection_point_names_a_real_symbol() {
    assert_eq!(
        frontend_bridge::INJECTION_POINT,
        "crate::platform::adapter::PlatformAdapter::set_bridge_injection"
    );
    let adapter_source = crate::platform::production_source(include_str!("adapter.rs"));
    assert!(
        adapter_source.contains("pub fn set_bridge_injection"),
        "INJECTION_POINT 指向的符号必须真实存在于 adapter.rs"
    );
}

/// 结构性证据：本模块不含任何 `ConnectionUseCases` 实现，也不出现 `todo!` /
/// `unimplemented!` / `unreachable!` 之类会把半成品伪装成完成的写法。
#[test]
fn no_shell_implementation_of_the_thirteen_use_cases_exists() {
    let source = include_str!("adapter.rs");
    let production = source.split("#[cfg(test)]").next().unwrap_or_default();
    assert!(
        !production.contains("impl ConnectionUseCases"),
        "本 track 不实现 13 个用例：实现就得编造 revision / epoch / 幂等值"
    );
    for forbidden in ["todo!", "unimplemented!", "TODO", "FIXME"] {
        assert!(
            !production.contains(forbidden),
            "adapter.rs 生产路径不得出现 {forbidden}"
        );
    }
}

/// :84 条款一的**语义**证据：`ProfileView` 的默认 revision 就是常量 0。
/// 这正是"伪映射"会落进去的洞：把常量 0 当成真实版本号填进去，
/// `ConfigRevisionMismatch` 判定会永久失效。本测试把这个事实钉住。
#[test]
fn profile_view_defaults_are_placeholders_and_must_not_be_used_on_the_desktop_path() {
    use datazen_platform_api::dto::profile::ProfileView;
    use datazen_platform_api::id::{ConnectionId, Counter};

    let view = ProfileView::new(ConnectionId::new("c-1"), "本地", "postgres");
    // 没有任何真实版本源时，唯一能拿到的就是 ZERO —— 一个编造值。
    assert_eq!(view.config_revision, Counter::ZERO);
    assert_eq!(view.credential_revision, Counter::ZERO);
    // 两个 revision 因此永远相等 ⇒ 版本不一致判定不可用。
    assert_eq!(view.config_revision, view.credential_revision);
    assert!(view.enabled);
    let real = view
        .with_config_revision(Counter::new(7))
        .with_credential_revision(Counter::new(3));
    assert_ne!(real.config_revision, real.credential_revision);
}

/// `inject` 注释写下的失败半径，钉成可执行断言。
///
/// `adapter.rs` 里 `inject` 的注释写明：装配失败后，依赖平台的访问器返回的
/// `ApiError` 码**多数**是 `ServiceUnavailable`——`runtime()` / `profiles()` /
/// `live_source()` / `artifact_store()` / `identity_resolver()` 这 5 个都是它；
/// `require_application_services()` 与 `native_tools()` 是 `CapabilityUnsupported`。
/// 两者在 IPC 表面都收敛成 `CommandError::NotConfigured`
/// （`platform/error.rs` 的窄适配）。
///
/// 这条把那句话从**注释**变成**断言**：改任何一个访问器的错误码、或把窄适配的
/// 收敛档位改掉，本条都会红。注释本身不��被验，行为才是。
#[test]
fn 未注入时各访问器的错误码与注释写的一致() {
    let adapter = adapter();

    let service_unavailable: Vec<(&str, Result<(), ApiError>)> = vec![
        ("artifact_store", adapter.artifact_store().map(|_| ())),
        ("live_source", adapter.live_source().map(|_| ())),
        ("runtime", adapter.runtime().map(|_| ())),
        ("profiles", adapter.profiles().map(|_| ())),
        ("identity_resolver", adapter.identity_resolver().map(|_| ())),
    ];
    for (name, result) in service_unavailable {
        let err = result
            .err()
            .unwrap_or_else(|| panic!("{name} 未注入必须报错"));
        assert_eq!(
            err.code,
            ApiErrorCode::ServiceUnavailable,
            "{name} 未注入时的 ApiError 码必须是 ServiceUnavailable"
        );
    }

    let capability_unsupported: Vec<(&str, Result<(), ApiError>)> = vec![
        (
            "require_application_services",
            adapter.require_application_services().map(|_| ()),
        ),
        ("native_tools", adapter.native_tools().map(|_| ())),
    ];
    for (name, result) in capability_unsupported {
        let err = result
            .err()
            .unwrap_or_else(|| panic!("{name} 未注入必须报错"));
        assert_eq!(
            err.code,
            ApiErrorCode::CapabilityUnsupported,
            "{name} 未注入时的 ApiError 码必须是 CapabilityUnsupported"
        );
    }

    // IPC 表面：两个码都收敛到 `NotConfigured`，前端拿到的分类一致。
    for code in [
        ApiErrorCode::ServiceUnavailable,
        ApiErrorCode::CapabilityUnsupported,
    ] {
        let mapped = crate::platform::error::into_command_error(ApiError::new(code, "boom"));
        assert!(
            matches!(mapped, crate::commands::CommandError::NotConfigured(_)),
            "{code:?} 在 IPC 表面必须收敛到 NotConfigured，实际 {mapped:?}"
        );
    }
}
