//! CM-72 能力与控制审计条目。
//!
//! ## 三条硬约束
//!
//! 1. **处置与执行终态是两个命名空间**。`Outcome` 说的是「这次**注册侧控制**被怎么处置」，
//!    `ExecutionState` 说的是「执行现在是什么状态」。把两者合并成一个枚举，调用方就分不清
//!    「请求没被接受」和「执行跑了但失败了」——这两者的补救动作完全不同。
//! 2. **`effectOutcome` 不得被请求成功覆盖**。`ExecutionEffectOutcome::Completed` 只由
//!    物理层在执行终态里给出。一次成功的**取消请求**只说明「意图已登记」，
//!    它对已经部分落库的数据一无所知。因此取消条目的构造函数**根本不接收**
//!    `effectOutcome` 参数——用签名挡住，比用约定挡住可靠。
//! 3. **审计只带非敏感的能力版本**。条目里出现凭据、驱动侧句柄或精确 cancelHandle
//!    就等于把攻击面写进日志；[`CapabilityVersions`] 只有契约版本与 driver API 版本。

use serde::{Deserialize, Serialize};

use crate::connection::{DbSessionId, ExecutionState};

/// `ExecutionState` 的协议字面量。
///
/// `connection::execution` 没有提供 `as_str()`（Wave 1 冻结面），而审计条目要存字面量。
/// 这份映射因此必须被测试**逐字钉在 serde 输出上**（见
/// `state_literals_match_the_serde_wire_casing`），否则它就是漂移的第二份事实来源。
pub(crate) fn state_literal(state: ExecutionState) -> &'static str {
    match state {
        ExecutionState::Queued => "queued",
        ExecutionState::Running => "running",
        ExecutionState::CancelRequested => "cancelRequested",
        ExecutionState::Succeeded => "succeeded",
        ExecutionState::Failed => "failed",
        ExecutionState::Cancelled => "cancelled",
    }
}

/// 注册侧处置（**不是**执行终态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// 控制请求被完整执行。
    Succeeded,
    /// 请求在派发前被拒绝（未产生副作用）。
    Rejected,
    /// 请求执行了但结果不可判定（如回滚结果未知）。
    Undecided,
}

impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Rejected => "rejected",
            Self::Undecided => "undecided",
        }
    }
}

/// 非敏感的能力版本快照（CM-72）。
///
/// **只有版本号**。能力位、方言细节可以记（它们是能力探测的结论），
/// 凭据、attachment token、驱动侧物理句柄一个都不许进来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityVersions {
    /// 驱动契约版本（字符串，形如 `1` / `2024-11-01`）。
    pub contract: String,
    /// driver API crate 版本。
    pub driver_api: String,
}

impl CapabilityVersions {
    pub fn new(contract: impl Into<String>, driver_api: impl Into<String>) -> Self {
        Self {
            contract: contract.into(),
            driver_api: driver_api.into(),
        }
    }

    /// 这两个值是否**确实只是版本号**。
    ///
    /// 结构上没有凭据字段（见上面的字段表），所以这里查的是另一件事：
    /// 调用方把 `postgres://user:pw@host/db` 或一段 64 位十六进制摘要
    /// 当作「能力版本」塞进来时，它长得不像版本号。
    ///
    /// 判据刻意保守：非空、不超过 32 字符、不含空白与 `/:@=`。
    /// 宽松判据在这里等于没有判据——审计条目里混进 DSN 的代价，
    /// 正是 CM-72 要消灭的那一类泄漏。
    pub fn is_version_only(&self) -> bool {
        fn looks_like_version(value: &str) -> bool {
            let trimmed = value.trim();
            !trimmed.is_empty()
                && trimmed.len() <= 32
                && !trimmed
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, '/' | ':' | '@' | '=' | '?' | '#'))
        }
        looks_like_version(&self.contract) && looks_like_version(&self.driver_api)
    }
}

/// 审计事件种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditKind {
    /// §7.1 会话登记成功（物理资源已建立且已进入 Routable）。
    SessionRegistered,
    /// §4.1 登记失败：调用方拿不到成功。
    SessionRegistrationFailed,
    /// §6.5 一批句柄登记到 actor。
    HandlesRegistered,
    /// §7.5 / §9.4 一批句柄在**原资源**上被终结。
    HandlesFinalized,
    /// 一次执行的终态。
    ExecutionCompleted,
    /// §7.6 一次取消请求的处置。
    CancelResolved,
    /// §6.4 空闲驱逐。
    SessionEvicted,
    /// §7.5 主动关闭。
    SessionClosed,
    /// §12 worker 崩溃导致整批会话作废。
    SessionInvalidated,
    /// §9.3 陈旧配额被扣住（尚不可复用）。
    QuotaHeldStale,
    /// §9.3 陈旧配额在隔离确认后释放。
    QuotaReleasedStale,
}

impl AuditKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionRegistered => "sessionRegistered",
            Self::SessionRegistrationFailed => "sessionRegistrationFailed",
            Self::HandlesRegistered => "handlesRegistered",
            Self::HandlesFinalized => "handlesFinalized",
            Self::ExecutionCompleted => "executionCompleted",
            Self::CancelResolved => "cancelResolved",
            Self::SessionEvicted => "sessionEvicted",
            Self::SessionClosed => "sessionClosed",
            Self::SessionInvalidated => "sessionInvalidated",
            Self::QuotaHeldStale => "quotaHeldStale",
            Self::QuotaReleasedStale => "quotaReleasedStale",
        }
    }
}

/// 一条审计条目。
///
/// 所有对外码都存**字面量**而不是枚举：审计的下游是日志与离线分析，
/// 一旦枚举改名而字面量没改，线上看到的是历史串；反过来一旦字面量被就地改掉，
/// 历史条目就再也读不出来。存字面量 + 由 [`super::epoch::fold_exit`] 唯一产出，
/// 才能保证「审计里的码」和「调用方收到的码」逐字相同。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryAuditEntry {
    pub kind: AuditKind,
    pub db_session_id: DbSessionId,
    pub runtime_epoch: u64,
    /// 注册侧处置。与 `execution_state` 正交。
    pub outcome: Outcome,
    /// 对外错误码字面量（`sessionNotFound` 等）。由折叠函数产出，本层不自造。
    pub error_code: Option<&'static str>,
    /// 取消处置字面量（`requested` / `unsupported` / `alreadyFinished`）。
    pub disposition: Option<&'static str>,
    /// 执行终态字面量（`succeeded` / `failed` / `cancelled` / …）。
    pub execution_state: Option<&'static str>,
    /// 物理层判定的效果。**只有执行终态条目可以带它**。
    pub effect_outcome: Option<&'static str>,
    pub capability_versions: Option<CapabilityVersions>,
    /// 该时刻 actor 内登记的句柄数（§9.4 归池前检查的宿主侧依据）。
    pub handle_count: usize,
}

impl RegistryAuditEntry {
    fn base(kind: AuditKind, db_session_id: DbSessionId, runtime_epoch: u64) -> Self {
        Self {
            kind,
            db_session_id,
            runtime_epoch,
            outcome: Outcome::Succeeded,
            error_code: None,
            disposition: None,
            execution_state: None,
            effect_outcome: None,
            capability_versions: None,
            handle_count: 0,
        }
    }

    /// 登记成功条目。带能力版本，便于事后回答「当时跑的 driver 契约是哪个」。
    pub fn registered(
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        capabilities: CapabilityVersions,
    ) -> Self {
        Self {
            capability_versions: Some(capabilities),
            ..Self::base(AuditKind::SessionRegistered, db_session_id, runtime_epoch)
        }
    }

    /// 登记失败条目：`error_code` 是折叠后的对外码，调用方拿到的就是它。
    pub fn registration_failed(
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        error_code: &'static str,
    ) -> Self {
        Self {
            outcome: Outcome::Rejected,
            error_code: Some(error_code),
            ..Self::base(
                AuditKind::SessionRegistrationFailed,
                db_session_id,
                runtime_epoch,
            )
        }
    }

    /// 执行终态条目。**这是唯一携带 `effect_outcome` 的入口**。
    pub fn execution_completed(
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        state: ExecutionState,
        effect_outcome: Option<&'static str>,
        handle_count: usize,
    ) -> Self {
        Self {
            outcome: Outcome::Succeeded,
            execution_state: Some(state_literal(state)),
            effect_outcome,
            handle_count,
            ..Self::base(AuditKind::ExecutionCompleted, db_session_id, runtime_epoch)
        }
    }

    /// 取消处置条目：`outcome` 是**宿主处理这次控制请求**的结论，不是执行结论。
    ///
    /// 取消请求被正常受理 → `succeeded`；绑定对不上被拒 → 走 [`Self::failure`] 记 `rejected`。
    /// 它与 `executionState`（执行被推到了哪个终态）分属两个键，
    /// 因此 `outcome = "succeeded"` + `executionState = "cancelRequested"` 这组值是自洽的，
    /// 不构成「执行成功了」的断言。
    ///
    /// 特别地：签名里**没有** `effect_outcome` 参数——取消请求成功绝不改写数据效果（CM-72）；
    /// 让调用方能顺手写一个进去，就等于允许覆盖物理层的判定。
    ///
    /// [`Self::failure`]: Self::failure
    pub fn cancel_resolved(
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        disposition: &'static str,
        state: ExecutionState,
        handle_count: usize,
    ) -> Self {
        Self {
            disposition: Some(disposition),
            execution_state: Some(state_literal(state)),
            handle_count,
            ..Self::base(AuditKind::CancelResolved, db_session_id, runtime_epoch)
        }
    }

    /// 句柄终结条目：`undecided` 对应 §9.4「回滚结果不可判定」。
    pub fn handles_finalized(
        kind: AuditKind,
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        outcome: Outcome,
        handle_count: usize,
    ) -> Self {
        Self {
            outcome,
            handle_count,
            ..Self::base(kind, db_session_id, runtime_epoch)
        }
    }

    /// 通用失败条目（驱逐、关闭、作废共用）。
    pub fn failure(
        kind: AuditKind,
        db_session_id: DbSessionId,
        runtime_epoch: u64,
        outcome: Outcome,
        error_code: Option<&'static str>,
    ) -> Self {
        Self {
            outcome,
            error_code,
            ..Self::base(kind, db_session_id, runtime_epoch)
        }
    }
}

/// 进程内审计日志。
///
/// **只在内存里**：它是排障与验收证据，不是合规存档，§12 明确禁止把任何会话态落盘。
#[derive(Debug, Clone, Default)]
pub struct AuditLog {
    entries: Vec<RegistryAuditEntry>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, entry: RegistryAuditEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[RegistryAuditEntry] {
        &self.entries
    }

    /// 某一次执行的效果判定（最后一次写入者胜出）。
    ///
    /// 取消条目**不会**出现在结果里——它从不写 `effect_outcome`。
    /// 这正是 CM-72 要的行为：一次成功的取消请求不得改写已判定的效果。
    pub fn effect_outcome_of(&self, db_session_id: &DbSessionId) -> Option<&'static str> {
        self.entries
            .iter()
            .filter(|entry| entry.db_session_id == *db_session_id)
            .filter_map(|entry| entry.effect_outcome)
            .next_back()
    }

    /// 某会话的全部条目（排障用）。
    pub fn for_session(&self, db_session_id: &DbSessionId) -> Vec<&RegistryAuditEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.db_session_id == *db_session_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::port::CancelDisposition;
    use crate::connection::EffectOutcome;

    fn session() -> DbSessionId {
        DbSessionId::new("db_1")
    }

    /// CM-72：取消**处置**字面量与执行**终态**字面量必须互不相交。
    ///
    /// 这一点成立且被逐字钉死：`requested` / `unsupported` / `alreadyFinished`
    /// 说的是「控制请求被怎么处置」，`queued` … `cancelled` 说的是「执行落到哪个终态」，
    /// 两组串没有任何交集。搞混的后果是调用方把「驱动不支持取消」读成「执行被取消了」。
    #[test]
    fn cancel_dispositions_never_read_like_execution_states() {
        let dispositions = [
            CancelDisposition::Requested.as_str(),
            CancelDisposition::Unsupported.as_str(),
            CancelDisposition::AlreadyFinished.as_str(),
        ];
        let states = [
            ExecutionState::Queued,
            ExecutionState::Running,
            ExecutionState::CancelRequested,
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ];
        for disposition in dispositions {
            for state in states {
                let literal = state_literal(state);
                assert_ne!(
                    disposition, literal,
                    "处置 {disposition:?} 与执行终态 {literal:?} 撞了，两个命名空间必须正交"
                );
            }
        }
    }

    /// CM-72：`outcome` 与 `executionState` 正交，靠的是**键名**不是字面量。
    ///
    /// 这里修正一个很容易写错的说法：两组的字面量集合**并不是**互不相交的——
    /// `Outcome::Succeeded` 和 `ExecutionState::Succeeded` 都序列化成 `"succeeded"`，
    /// 冻结的枚举已经如此，registry 改不了。真正被禁止的是「读错键」：
    /// 下游任何时候都得能分辨某个值是控制侧处置还是执行终态，
    /// 而这条保证来自两个互不相干的键，不来自串本身。
    #[test]
    fn control_disposition_and_execution_state_live_in_separate_fields() {
        let cancel = RegistryAuditEntry::cancel_resolved(
            session(),
            3,
            CancelDisposition::Requested.as_str(),
            ExecutionState::CancelRequested,
            0,
        );
        let value: serde_json::Value = serde_json::to_value(&cancel).expect("可序列化");
        // outcome=succeeded 说的是「宿主把这次控制请求处理完了」，与执行终态无关。
        assert_eq!(value["outcome"], serde_json::json!("succeeded"));
        assert_eq!(
            value["executionState"],
            serde_json::json!("cancelRequested")
        );
        assert_eq!(value["disposition"], serde_json::json!("requested"));
        // 执行终态字面量绝不写进 outcome，处置字面量绝不写进 executionState。
        assert_ne!(value["outcome"], value["executionState"]);

        // 一条成功执行的条目：outcome=succeeded 与 executionState=succeeded
        // 恰好同串，但落在两个键上——这正是必须靠键名而不是靠串区分的原因。
        let done = RegistryAuditEntry::execution_completed(
            session(),
            3,
            ExecutionState::Succeeded,
            Some(EffectOutcome::Completed.as_str()),
            0,
        );
        let value: serde_json::Value = serde_json::to_value(&done).expect("可序列化");
        assert_eq!(value["outcome"], serde_json::json!("succeeded"));
        assert_eq!(value["executionState"], serde_json::json!("succeeded"));
        assert_eq!(
            value["outcome"].as_str(),
            value["executionState"].as_str(),
            "两组字面量确实会在 succeeded 上重合（冻结枚举如此），分离完全由键名保证"
        );
    }

    /// CM-72：取消条目**不可能**携带 `effectOutcome`。
    ///
    /// 用构造函数的签名来证明：它压根没有这个参数。
    #[test]
    fn cancel_entry_constructor_cannot_carry_an_effect_outcome() {
        let entry = RegistryAuditEntry::cancel_resolved(
            session(),
            3,
            "requested",
            ExecutionState::CancelRequested,
            2,
        );
        assert_eq!(entry.effect_outcome, None);
        assert_eq!(entry.disposition, Some("requested"));
        assert_eq!(entry.execution_state, Some("cancelRequested"));
    }

    /// CM-72 行为面：一次**成功**的取消请求不得改写已判定的效果。
    ///
    /// 这是本模块最容易写错的一条——把「取消成功」顺手写成 `completed`，
    /// 就等于宣称一次部分落库的数据已经干净了。
    #[test]
    fn successful_cancel_never_overwrites_the_recorded_effect_outcome() {
        let mut log = AuditLog::new();
        log.record(RegistryAuditEntry::execution_completed(
            session(),
            3,
            ExecutionState::Failed,
            Some("partiallyApplied"),
            1,
        ));
        log.record(RegistryAuditEntry::cancel_resolved(
            session(),
            3,
            "requested",
            ExecutionState::CancelRequested,
            1,
        ));
        assert_eq!(
            log.effect_outcome_of(&session()),
            Some("partiallyApplied"),
            "取消请求成功不代表数据已落库或已回滚；效果判定必须原样保留"
        );
        // 两次都成功，但两次的成功含义不同：审计里必须能看出区别。
        let entries = log.for_session(&session());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].effect_outcome, Some("partiallyApplied"));
        assert_eq!(entries[1].effect_outcome, None);
    }

    /// CM-72：对外错误码在序列化后**逐字**等于枚举字面量。
    ///
    /// 断言的是字面量而不是枚举相等——枚举相等在 `rename_all` 被误改时照样通过。
    #[test]
    fn error_codes_serialize_with_their_exact_wire_casing() {
        let entry = RegistryAuditEntry::registration_failed(session(), 3, "sessionNotFound");
        let json = serde_json::to_string(&entry).expect("可序列化");
        assert!(
            json.contains(r#""errorCode":"sessionNotFound""#),
            "错误码字面量必须在序列化后逐字不变：{json}"
        );
        assert!(
            json.contains(r#""kind":"sessionRegistrationFailed""#),
            "事件种类同样走 camelCase：{json}"
        );
        // 条目里的错误码存的是 `&'static str`（线上字面量）。
        // 这带来一个**真实**的类型层后果：手写 `Deserialize` 对 `&'static str`
        // 要求 `'de: 'static`，因此 `serde_json::from_str` / `from_value`
        // 在这个类型上都编译不过（"implementation of Deserialize is not general enough"）。
        // 本模块不打算把线上字面量改成 `String`——那样 CM-72 要求的
        // 「审计侧与调用方逐字同源」就退化成一个可以随手写错的普通字符串。
        // 所以反序列化一侧改用值树逐键断言，同样是逐字校验，且不触碰线上形状。
        let value: serde_json::Value = serde_json::from_str(&json).expect("可反序列化");
        assert_eq!(value["errorCode"], serde_json::json!("sessionNotFound"));
        assert_eq!(
            value["kind"],
            serde_json::json!("sessionRegistrationFailed")
        );
        assert_eq!(value["dbSessionId"], serde_json::json!("db_1"));
        assert_eq!(value["outcome"], serde_json::json!("rejected"));
    }

    /// CM-72：能力版本只有版本号，条目里不得出现凭据或物理句柄。
    #[test]
    fn audit_entries_never_carry_credentials_or_physical_handles() {
        let capabilities = CapabilityVersions::new("1", "0.0.9");
        assert!(capabilities.is_version_only());
        // 反例：把 DSN 当版本号塞进来的形状必须被挡住。
        assert!(!CapabilityVersions::new("postgres://user:pw@host/db", "0.0.9").is_version_only());
        assert!(!CapabilityVersions::new("1", "  ").is_version_only());
        let entry = RegistryAuditEntry::registered(session(), 3, capabilities);
        let json = serde_json::to_string(&entry).expect("可序列化");
        for forbidden in [
            "password",
            "secret",
            "token",
            "attachmentToken",
            "resourceId",
            "cancelHandle",
            "ownerToken",
        ] {
            assert!(
                !json.contains(forbidden),
                "审计条目泄露了 {forbidden}：{json}"
            );
        }
        assert!(json.contains(r#""capabilityVersions":{"contract":"1","driverApi":"0.0.9"}"#));
    }

    /// 本模块私有的执行状态字面量映射必须与 serde 输出**逐字一致**。
    ///
    /// 这是防止「第二份事实来源」的手段：映射表是手写的，
    /// 只有这条断言能发现 `ExecutionState` 改名后映射表没跟着改。
    #[test]
    fn state_literals_match_the_serde_wire_casing() {
        for state in [
            ExecutionState::Queued,
            ExecutionState::Running,
            ExecutionState::CancelRequested,
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ] {
            assert_eq!(
                serde_json::to_string(&state).expect("可序列化"),
                format!("\"{}\"", state_literal(state)),
                "审计里记的执行状态字面量必须与线上字面量一致"
            );
        }
    }

    /// 处置为「不可判定」时必须被如实记录，不能被压成成功（§9.4 回滚结果未知）。
    #[test]
    fn undecided_finalization_is_recorded_as_undecided() {
        let mut log = AuditLog::new();
        log.record(RegistryAuditEntry::handles_finalized(
            AuditKind::HandlesFinalized,
            session(),
            3,
            Outcome::Undecided,
            1,
        ));
        let entry = log.for_session(&session())[0];
        assert_eq!(entry.outcome, Outcome::Undecided);
        assert_eq!(entry.outcome.as_str(), "undecided");
    }
}
