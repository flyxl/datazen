//! 新 frontend bridge 的注入缝隙（开发计划 :84 条款二 / 概要 §7.4）。
//!
//! ## 条款原文与本文件的对应
//!
//! > 新 frontend bridge 在未注入时**明确报错**。
//!
//! "明确报错"在这里被实现为**两段**，都失败关闭：
//!
//! 1. **运行期**：[`FrontendBridge::require`] 在未注入时返回固定、可定位的
//!    [`ApiErrorCode::CapabilityUnsupported`]，文案里带注入点名字
//!    （`set_bridge_injection`）和所在文件，绝不回落到"直接走 Tauri"。
//! 2. **类型期**：[`BridgeInjection`] 只有一个非空字段 [`FrontendBridge`]，
//!    没有"可空桥"这个状态在类型上可表示；`FrontendBridge` 只提供**窄端口**
//!    （`open_session` / `close_session`），不暴露 Tauri `AppHandle`。
//!
//! ## 为什么放在 Rust 侧
//!
//! `packages/backend-client` 在本 track 的基线提交上**不存在**（概要 §3「契约层已实现」
//! 对 TS 侧不成立）。因此 TS 侧的 `requireBackendClient(backendId?)` 属于另一条 track。
//! 本文件只提供 Rust 侧的注入缝隙与失败关闭行为：前端桥接真正落地时，
//! TS 实现在此处注入，Rust 侧不需要再改语义。
//!
//! ## 为什么不提供 `Option<FrontendBridge>` 直通
//!
//! 那样写会得到 `if let Some(b) = bridge { … } else { /* 旧路径 */ }` 的形状，
//! 即概要 §7.4 明令禁止的 "silent fallback"。本文件只暴露 `require`，
//! 让"未注入"在任何调用点都必须被显式处理。

use std::sync::{Arc, OnceLock};

use datazen_application::error::{ApiError, ApiErrorCode};

/// 注入点名称。出现在错误文案里，让报错能直接定位到这一行。
pub const INJECTION_POINT: &str = "crate::platform::adapter::PlatformAdapter::set_bridge_injection";

/// 前端桥的窄端口。刻意**不**含 `tauri::AppHandle`：桥只应表达"前端语义"，
/// 拿到 Tauri 句柄就等于拿到绕过本层的后门。
pub trait FrontendBridge: Send + Sync + 'static {
    /// 打开一个会话视图，返回前端可直接消费的 JSON 负载。
    ///
    /// 实现方**不得**在此处自行拼装 `RequestContext`：身份只由
    /// [`crate::platform::identity::DesktopIdentity`] 组装。
    fn open_session(&self, payload: &serde_json::Value) -> Result<serde_json::Value, ApiError>;

    /// 关闭会话。`None` 表示由桥自行判定；桌面形态必须实现为幂等。
    fn close_session(
        &self,
        db_session_id: &str,
        payload: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ApiError>;
}

/// 注入容器。字段非空 ⇒ 类型上不存在"半注入"状态。
///
/// 抽成独立类型而不是直接把 `OnceLock` 写成全局，是为了让"重复注入被拒绝"这条规则
/// 可以在**独立实例**上被确定性地测试——测试进程级全局 `OnceLock` 时，并行用例之间
/// 的先后顺序不可控，断言只能写成"两种结果都算过"，那等于没测。
#[derive(Clone)]
pub struct BridgeInjection {
    bridge: Arc<dyn FrontendBridge>,
}

impl BridgeInjection {
    pub fn new(bridge: Arc<dyn FrontendBridge>) -> Self {
        Self { bridge }
    }

    pub fn bridge(&self) -> &Arc<dyn FrontendBridge> {
        &self.bridge
    }
}

impl std::fmt::Debug for BridgeInjection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 桥的具体类型不必也不应进日志。
        f.debug_struct("BridgeInjection").finish_non_exhaustive()
    }
}

/// 只写一次的注入槽。
#[derive(Default)]
struct InjectionSlot {
    cell: OnceLock<BridgeInjection>,
}

impl InjectionSlot {
    const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }

    /// 首次写入返回 `true`；此后一律 `false` 且**不覆盖**。
    fn set(&self, injection: BridgeInjection) -> bool {
        if self.cell.set(injection).is_err() {
            tracing::warn!("前端桥已注入，忽略重复注入");
            return false;
        }
        tracing::info!("前端桥已注入");
        true
    }

    fn get(&self) -> Result<&Arc<dyn FrontendBridge>, ApiError> {
        self.cell
            .get()
            .map(BridgeInjection::bridge)
            .ok_or_else(not_injected)
    }
}

/// 进程级注入点。`OnceLock` ⇒ 注入一次后不可替换，避免运行期中途换桥导致语义漂移。
static INJECTION: InjectionSlot = InjectionSlot::new();

/// 注入前端桥。第二次注入返回 `false` 且**不覆盖**。
pub fn set_bridge_injection(injection: BridgeInjection) -> bool {
    INJECTION.set(injection)
}

/// 取出前端桥；未注入 ⇒ 明确报错。
///
/// 错误文案包含注入点符号名，可直接定位。**这里没有 `unwrap`、没有默认桥、
/// 没有 Tauri 直连兜底**。
pub fn require_bridge() -> Result<&'static Arc<dyn FrontendBridge>, ApiError> {
    INJECTION.get()
}

/// 未注入时的固定错误。单独成函数，便于测试断言文案稳定性。
pub fn not_injected() -> ApiError {
    ApiError::new(
        ApiErrorCode::CapabilityUnsupported,
        format!(
            "新 frontend bridge 未注入：在 {INJECTION_POINT} 调用 \
             set_bridge_injection 后再使用；未注入时不得回落到 Tauri 直连"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct RecordingBridge {
        opens: AtomicUsize,
        closes: AtomicUsize,
    }

    impl FrontendBridge for RecordingBridge {
        fn open_session(
            &self,
            _payload: &serde_json::Value,
        ) -> Result<serde_json::Value, ApiError> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({ "dbSessionId": "s-1" }))
        }

        fn close_session(
            &self,
            db_session_id: &str,
            _payload: Option<&serde_json::Value>,
        ) -> Result<serde_json::Value, ApiError> {
            self.closes.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({ "closed": db_session_id }))
        }
    }

    #[test]
    fn not_injected_error_is_explicit_and_locatable() {
        let err = not_injected();
        assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
        assert!(
            err.message.contains("未注入"),
            "错误文案必须说明未注入：{}",
            err.message
        );
        assert!(
            err.message.contains("set_bridge_injection"),
            "错误文案必须给出可定位的注入点：{}",
            err.message
        );
        assert!(
            err.message.contains(INJECTION_POINT),
            "错误文案必须带注入点符号名：{}",
            err.message
        );
        // §7.4：明令不得静默回落到 Tauri 直连。
        assert!(
            err.message.contains("不得回落到 Tauri 直连"),
            "错误文案必须禁止静默回落：{}",
            err.message
        );
    }

    /// 注入语义在**独立槽**上确定性验证：首次成功、重复被拒且不覆盖、生效的是第一个。
    #[test]
    fn injection_is_visible_once_set_and_not_replaceable() {
        let slot = InjectionSlot::new();
        let bridge = Arc::new(RecordingBridge {
            opens: AtomicUsize::new(0),
            closes: AtomicUsize::new(0),
        });
        assert!(
            slot.set(BridgeInjection::new(bridge.clone())),
            "首次注入应当成功"
        );

        let other = Arc::new(RecordingBridge {
            opens: AtomicUsize::new(0),
            closes: AtomicUsize::new(0),
        });
        assert!(
            !slot.set(BridgeInjection::new(other.clone())),
            "重复注入必须被拒绝"
        );

        // 生效的仍是第一次注入的那一个。
        slot.get()
            .expect("注入后应可取到")
            .open_session(&serde_json::json!({}))
            .expect("open");
        assert_eq!(bridge.opens.load(Ordering::SeqCst), 1);
        assert_eq!(other.opens.load(Ordering::SeqCst), 0);
    }

    /// 未注入时的行为同样在独立槽上验证，不受其它用例影响。
    #[test]
    fn an_uninjected_slot_errors_instead_of_holding_a_placeholder() {
        let slot = InjectionSlot::new();
        match slot.get() {
            Ok(_) => panic!("未注入时不得凭空取出桥"),
            Err(err) => {
                assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
                assert!(err.message.contains("未注入"));
            }
        }
    }

    /// 进程级入口：与其它用例共享 `OnceLock`，因此只断言"不会 panic、结果可归类"。
    #[test]
    fn process_wide_entry_point_is_usable_from_any_order() {
        set_bridge_injection(BridgeInjection::new(Arc::new(RecordingBridge {
            opens: AtomicUsize::new(0),
            closes: AtomicUsize::new(0),
        })));
        match require_bridge() {
            Ok(_) => {}
            Err(err) => assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported),
        }
        // 重复注入一律被拒绝——这一条与顺序无关。
        assert!(!set_bridge_injection(BridgeInjection::new(Arc::new(
            RecordingBridge {
                opens: AtomicUsize::new(0),
                closes: AtomicUsize::new(0),
            }
        ))));
    }

    /// 桥的转发必须原样返回结果与错误，不能吞。
    #[test]
    fn injected_bridge_forwards_without_swallowing_errors() {
        let slot = InjectionSlot::new();
        let bridge = Arc::new(RecordingBridge {
            opens: AtomicUsize::new(0),
            closes: AtomicUsize::new(0),
        });
        slot.set(BridgeInjection::new(bridge.clone()));

        let opened = slot
            .get()
            .expect("注入后应可取到")
            .open_session(&serde_json::json!({}))
            .expect("open");
        assert_eq!(opened["dbSessionId"], "s-1");

        let closed = slot
            .get()
            .expect("注入后应可取到")
            .close_session("s-1", None)
            .expect("close");
        assert_eq!(closed["closed"], "s-1");
        assert_eq!(bridge.closes.load(Ordering::SeqCst), 1);
    }

    /// 桥的错误必须原样透传：调用方要靠它区分失败原因。
    #[test]
    fn bridge_errors_reach_the_caller_unchanged() {
        struct FailingBridge;
        impl FrontendBridge for FailingBridge {
            fn open_session(
                &self,
                _payload: &serde_json::Value,
            ) -> Result<serde_json::Value, ApiError> {
                Err(ApiError::new(ApiErrorCode::SessionLost, "后端会话已丢失"))
            }

            fn close_session(
                &self,
                _db_session_id: &str,
                _payload: Option<&serde_json::Value>,
            ) -> Result<serde_json::Value, ApiError> {
                Err(ApiError::new(ApiErrorCode::TargetConflict, "目标已被占用"))
            }
        }

        let slot = InjectionSlot::new();
        slot.set(BridgeInjection::new(Arc::new(FailingBridge)));
        let bridge = slot.get().expect("注入后应可取到");
        assert_eq!(
            bridge
                .open_session(&serde_json::json!({}))
                .expect_err("必须透传")
                .code,
            ApiErrorCode::SessionLost
        );
        assert_eq!(
            bridge
                .close_session("s-1", None)
                .expect_err("必须透传")
                .code,
            ApiErrorCode::TargetConflict
        );
    }

    /// 类型层证据：注入容器不接受空桥，`FrontendBridge` 也不暴露 Tauri 句柄。
    ///
    /// 扫描的是**去掉注释后**的生产代码——模块文档里必须允许解释"为什么不暴露句柄"。
    #[test]
    fn bridge_trait_has_no_tauri_handle_and_no_optional_bridge_type() {
        let production = crate::platform::production_source(include_str!("bridge.rs"));
        assert!(
            !production.contains("AppHandle"),
            "桥端口不得暴露 Tauri 句柄，否则等于绕过本层的后门"
        );
        assert!(
            !production.contains("Option<FrontendBridge>"),
            "不得存在「可空桥」类型，否则调用点会退化成静默回落"
        );
        assert!(
            !production.contains("Option<BridgeInjection>"),
            "注入容器同样不得可空"
        );
    }

    /// 注入点常量必须指向真实符号，否则"可定位"的承诺是假的。
    #[test]
    fn injection_point_names_a_real_symbol() {
        assert_eq!(
            INJECTION_POINT,
            "crate::platform::adapter::PlatformAdapter::set_bridge_injection"
        );
        let adapter_source = crate::platform::production_source(include_str!("adapter.rs"));
        assert!(
            adapter_source.contains("pub fn set_bridge_injection"),
            "INJECTION_POINT 指向的符号必须真实存在于 adapter.rs"
        );
    }
}
