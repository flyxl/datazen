//! `CancelReceipt` —— §7.6 的取消回执（D-01 补齐的冻结面缺口）。
//!
//! **为什么它住在 `registry/` 而不是 `connection/`**：`connection/**` 是 Wave 1 冻结的
//! 会话/执行 DTO 层，端口的接缝由 Wave 2 的 registry 轨道实现；`SessionPort::cancel_execution`
//! 在冻结时退化成返回 `ExecutionState`（见 [`super::port`] 里 D-01 的说明），
//! 补齐它属于**端口实现侧**的职责，因此类型落在 `registry/` 内，由本模块对外导出。
//!
//! **`CancelDisposition` 是转出而不是重定义**：三态取值、协议字面量
//! （`requested` / `unsupported` / `alreadyFinished`）与 serde 形状都已冻结在
//! `crate::connection::port::CancelDisposition`，本模块只做 `pub use`。另起一份
//! 同名枚举会让「同一个字面量有两个 Rust 类型」，线上表现与类型系统一起失真。
//!
//! **三态的语义边界（§13.1）**：
//!
//! | disposition | 含义 | 必须是 `Err` 吗 |
//! | --- | --- | --- |
//! | `requested` | 取消意图已登记 | 否 |
//! | `unsupported` | 该 driver 的独立控制路径不可达（**正常返回**） | **否**——禁止抛 `CancellationUnsupported` 之类的异常 |
//! | `alreadyFinished` | 执行已是终态，终态优先不被改写 | 否 |
//!
//! **`state` 与 `disposition` 是正交的两个命名空间**：`disposition` 说的是「这次控制请求
//! 得到了什么处置」，`state` 说的是「执行现在是什么状态」。取消失败绝不允许塌缩成
//! 「已取消」：驱动不可达是 `unsupported` 而不是 `Cancelled`，绑定对不上是 `Err`。

use serde::{Deserialize, Serialize};

use crate::connection::{ExecutionId, ExecutionState};

/// 协议字面量与 serde 形状的**转出**，不是第二份定义。见模块文档的「转出而不是重定义」。
///
/// 之所以在这里 `pub use` 一次：§7.6 的三态与 `CancelReceipt` 同属取消这一个概念，
/// 调用方 `use runtime::registry::CancelDisposition` 就能拿到完整的一对，
/// 而不必知道三态实际冻结在 `connection::port` 里。
#[doc(inline)]
pub use crate::connection::port::CancelDisposition;

/// `ExecutionState` 的终态判据（§7.6「终态优先」）。
///
/// `connection::execution` 没有提供这个谓词，而本模块的两条规则都依赖它；
/// 私有定义是为了避免为它改动 Wave 1 冻结文件。
const fn is_terminal(state: ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
    )
}

/// §7.6 规定的取消回执：`executionId` + `disposition` + `state`。
///
/// 三个字段缺一不可：缺 `disposition` 就分不清「不支持取消」与「已是终态」，
/// 缺 `state` 调用方就得再读一次会话投影才能知道结果，竞态窗口从回执挪到了调用方身上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    /// 被取消的那一次执行。**即使处置是 `unsupported` 或 `alreadyFinished` 也必须回填**：
    /// 调用方需要它把控制请求与执行记录对上，而回执本身就是这条对应关系。
    pub execution_id: ExecutionId,
    /// 控制请求得到的处置。
    pub disposition: CancelDisposition,
    /// 请求落地后的执行状态快照（§7.6 末段：终态只能由终态事件给出，
    /// 本字段是**快照**而不是终态承诺）。
    pub state: ExecutionState,
}

impl CancelReceipt {
    /// 按 §7.6 的**终态优先**规则归一化一次取消尝试的观测结果。
    ///
    /// 规则只有两条，且顺序不可换：
    ///
    /// 1. 执行已是终态（`Succeeded`/`Failed`/`Cancelled`）→ 无论 driver 支不支持取消、
    ///    绑定对不对得上，一律 `AlreadyFinished` 并原样回填终态。终态不是被取消写出来的，
    ///    把它改写成 `CancelRequested` 等于凭空捏造一次未发生的干预。
    /// 2. 非终态下 driver 不支持独立取消路径 → `Unsupported`，**状态保持原样**
    ///    （`Running` 仍是 `Running`）。把「不支持」写成 `CancelRequested` 是本模块
    ///    最容易被违反的一条：调用方会据此以为「正在取消中」。
    ///
    /// `driver_supports_cancel` 只表达 driver 的独立控制路径是否可达；
    /// 绑定对不上根本走不到本函数——它在到达后端之前就被 registry 挡掉并返回 `Err`
    /// （[`super::actor`] 的取消路径），因为伪造的绑定必须**不产生任何后端调用**。
    pub fn normalize(
        execution_id: ExecutionId,
        observed_state: ExecutionState,
        driver_supports_cancel: bool,
    ) -> Self {
        let disposition = if is_terminal(observed_state) {
            CancelDisposition::AlreadyFinished
        } else if driver_supports_cancel {
            CancelDisposition::Requested
        } else {
            CancelDisposition::Unsupported
        };
        Self {
            execution_id,
            disposition,
            state: observed_state,
        }
    }

    /// `disposition` 与 `state` 是否自洽（登记与审计用；不自洽的组合不允许被构造出来）。
    pub const fn is_coherent(&self) -> bool {
        match self.disposition {
            CancelDisposition::AlreadyFinished => is_terminal(self.state),
            // `Requested` / `Unsupported` 都必须处在非终态：终态由规则 1 独占。
            CancelDisposition::Requested | CancelDisposition::Unsupported => {
                !is_terminal(self.state)
            }
        }
    }

    /// 是否真的登记了一次取消意图（`unsupported` / `alreadyFinished` 都不是）。
    pub const fn is_requested(&self) -> bool {
        matches!(self.disposition, CancelDisposition::Requested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(name: &str) -> ExecutionId {
        ExecutionId::new(name)
    }

    /// D-01 的形状约束：三个字段齐全，且 camelCase 序列化后逐字等于 §7.6 的键名。
    ///
    /// 断言的是**字面量**而不是枚举相等——枚举相等在 `#[serde(rename_all = "camelCase")]`
    /// 被误删时照样通过，而线上键名会全变成 snake_case。
    #[test]
    fn receipt_serializes_with_the_frozen_camel_case_keys() {
        let receipt = CancelReceipt::normalize(exec("exec_1"), ExecutionState::Running, true);
        let json = serde_json::to_string(&receipt).expect("回执必须可序列化");
        assert_eq!(
            json,
            r#"{"executionId":"exec_1","disposition":"requested","state":"running"}"#
        );
        // 键集合逐字钉死：`disposition` 本身就是单词，camelCase 下它**不该**变。
        let value: serde_json::Value = serde_json::from_str(&json).expect("可反序列化");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("回执必须是对象")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["disposition", "executionId", "state"]);
        // 不能对整段 JSON 扫下划线——`executionId` 的**值**（这里恰好是 `exec_1`）
        // 本来就可能含下划线，那与键名无关。键名已经被上面的键集合逐字钉死。
        assert!(
            keys.iter().all(|k| !k.contains('_')),
            "键名里不允许出现下划线（snake_case）：{keys:?}"
        );
    }

    /// 三态字面量逐字钉死：`requested` / `unsupported` / `alreadyFinished`（§13.1）。
    #[test]
    fn dispositions_keep_their_protocol_literals() {
        assert_eq!(CancelDisposition::Requested.as_str(), "requested");
        assert_eq!(CancelDisposition::Unsupported.as_str(), "unsupported");
        assert_eq!(
            CancelDisposition::AlreadyFinished.as_str(),
            "alreadyFinished"
        );
    }

    /// §7.6 终态优先：对已终态的执行重复取消**不得**改写终态（CM-23）。
    ///
    /// 反例一旦成立就是谎报：调用方会把一次没有发生过的干预当成发生过。
    #[test]
    fn terminal_execution_is_never_rewritten_as_cancel_requested() {
        for terminal in [
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ] {
            for supports in [true, false] {
                let receipt = CancelReceipt::normalize(exec("exec_1"), terminal, supports);
                assert_eq!(
                    receipt.disposition,
                    CancelDisposition::AlreadyFinished,
                    "{terminal:?} + driver_supports={supports} 必须报 alreadyFinished"
                );
                assert_eq!(receipt.state, terminal, "{terminal:?} 的终态必须原样回填");
                assert!(receipt.is_coherent());
                assert!(!receipt.is_requested());
            }
        }
    }

    /// CM-22：driver 没有独立取消路径时 `unsupported` 是**正常返回**，
    /// 状态保持 `Running`，绝不能被写成 `CancelRequested`。
    #[test]
    fn driver_without_cancel_path_reports_unsupported_without_touching_state() {
        let receipt = CancelReceipt::normalize(exec("exec_1"), ExecutionState::Running, false);
        assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
        assert_eq!(
            receipt.state,
            ExecutionState::Running,
            "不支持取消不等于取消中；状态必须保持原样"
        );
        assert!(receipt.is_coherent());
        assert!(!receipt.is_requested());
        // 关键：它是一次 `Ok` 的回执，而不是异常。回执本身不带 Result 语义。
        assert_eq!(
            receipt.execution_id,
            exec("exec_1"),
            "即便不支持取消，也必须回填 executionId 供调用方对账"
        );
    }

    /// 非终态 + 驱动支持取消 → `requested`，且状态推进到 `CancelRequested`（登记意图）。
    #[test]
    fn supported_driver_registers_intent_and_returns_cancel_requested() {
        let receipt = CancelReceipt::normalize(exec("exec_1"), ExecutionState::Running, true);
        assert_eq!(receipt.disposition, CancelDisposition::Requested);
        assert!(receipt.is_requested());
        assert!(receipt.is_coherent());
    }

    /// 九种 (state × 支撑) 组合逐一核对自洽性，把规则钉在**全表**上而不是抽样。
    #[test]
    fn every_state_and_support_combination_is_coherent() {
        let states = [
            ExecutionState::Queued,
            ExecutionState::Running,
            ExecutionState::CancelRequested,
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ];
        let mut checked = 0usize;
        for state in states {
            for supports in [true, false] {
                let receipt = CancelReceipt::normalize(exec("exec_1"), state, supports);
                assert!(
                    receipt.is_coherent(),
                    "{state:?} + supports={supports} 产出不自洽回执 {receipt:?}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 12, "6 个状态 × 2 种驱动能力必须全覆盖");
    }

    /// 反序列化对称：线上收到的回执要能原样读回来（字面量层面）。
    #[test]
    fn round_trips_through_json_unchanged() {
        let receipt = CancelReceipt::normalize(exec("exec_9"), ExecutionState::Queued, false);
        let encoded = serde_json::to_string(&receipt).expect("可序列化");
        let decoded: CancelReceipt = serde_json::from_str(&encoded).expect("可反序列化");
        assert_eq!(decoded, receipt);
        assert!(encoded.contains(r#""disposition":"unsupported""#));
    }
}
