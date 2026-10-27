//! 字节账（§6.2）：读页之前先预留额度，账满即缩窗或明确拒绝。
//!
//! - 账上同时活着两种副本：读回来的解码行、IR 转换后的行（待发送参数按同一
//!   量级预留），所以一行的预算是 [`PIPELINE_ROW_LIVE_COPIES`] 份。
//! - [`PipelineBudget::try_account`] 只在不超过容量时记账：拒绝的额度从不进入
//!   `used`，因此 `max_used` 永远不越过容量，峰值才是可断言的事实。
//! - 行字节估计先给种值，读到真实页之后由 [`PipelineBudget::observe_page`] 收敛
//!   （取最近实测与上次衰减的较大者，避免一次小页把后续预留估得过低）。

use datazen_driver_api::Value;

/// 有界管道缓冲初值（§6.2）。
pub const PIPELINE_INITIAL_BYTES: usize = 8 * 1024 * 1024;

/// 一行在管道里同时存活的副本数：读回来的解码行 + 转换后的行（参数批次与转换
/// 副本同量级，共用这一份预留）。
pub const PIPELINE_ROW_LIVE_COPIES: usize = 2;

/// 行字节估计的首值：还没有样本时按每行 1 KiB 估，读到真实页后立刻收敛。
const PIPELINE_ROW_BYTES_SEED: usize = 1024;

/// 行字节估计的衰减分母：上次估计按 1/8 退，避免历史大行永久压着后续预留。
const PIPELINE_ROW_DECAY_DIVISOR: usize = 8;

/// 字节账：解码行 + 转换副本 + 待发送参数；容量 8 MiB 初值。
#[derive(Debug, Clone)]
pub struct PipelineBudget {
    capacity: usize,
    used: usize,
    max_used: usize,
    row_bytes: usize,
}

impl PipelineBudget {
    pub fn new() -> Self {
        Self {
            capacity: PIPELINE_INITIAL_BYTES,
            used: 0,
            max_used: 0,
            row_bytes: PIPELINE_ROW_BYTES_SEED,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn max_used(&self) -> usize {
        self.max_used
    }

    /// 当前每行预估值（含 [`PIPELINE_ROW_LIVE_COPIES`] 份副本）。
    pub fn row_bytes(&self) -> usize {
        self.row_bytes.saturating_mul(PIPELINE_ROW_LIVE_COPIES)
    }

    /// 记一笔已发生的占用。超过容量返回 `false` 且**不记账**，账面与峰值都保持
    /// 在容量以内——拒绝必须发生在入账之前。
    pub fn try_account(&mut self, bytes: usize) -> bool {
        let next = match self.used.checked_add(bytes) {
            Some(next) if next <= self.capacity => next,
            _ => return false,
        };
        self.used = next;
        self.max_used = self.max_used.max(next);
        true
    }

    /// 读页前的预留：按行数 × 存活副本数先把额度占住，读回来再按实际结算。
    ///
    /// 预留本身也受容量约束：额度不够时只占满剩余额度，于是这一页读回来必然
    /// 触发 [`Self::try_account`] 拒绝，由调用方缩窗或明确失败——缓冲不会为了
    /// 塞下这一页而扩张。
    pub fn reserve_page(&mut self, rows: u32) -> usize {
        let asked = rows as usize * self.row_bytes();
        let bounded = asked.min(self.capacity.saturating_sub(self.used));
        self.used += bounded;
        self.max_used = self.max_used.max(self.used);
        bounded
    }

    /// 收敛行字节估计：取最近一次实测与上次估计衰减后的较大者。
    pub fn observe_page(&mut self, page_bytes: usize, rows: usize) {
        if rows == 0 {
            return;
        }
        let latest = (page_bytes / rows).max(1);
        let decayed = self
            .row_bytes
            .saturating_sub(self.row_bytes / PIPELINE_ROW_DECAY_DIVISOR);
        self.row_bytes = latest.max(decayed);
    }

    /// 这一轮读多少行：受剩余额度与请求上限约束，且至少 1 行——读 1 行仍超限
    /// 时由调用方明确失败，不静默扩张缓冲。
    pub fn affordable_rows(&self, requested: u32) -> u32 {
        let headroom = self.capacity.saturating_sub(self.used);
        requested
            .min((headroom / self.row_bytes().max(1)) as u32)
            .max(1)
    }

    pub fn release(&mut self, bytes: usize) {
        self.used = self.used.saturating_sub(bytes);
    }
}

impl Default for PipelineBudget {
    fn default() -> Self {
        Self::new()
    }
}

/// 单值字节估计（计入缓冲账）：按编码后的实际大小计，不给 JSON/Timestamp
/// 之类变长值留固定 16 字节的便宜——那正是超限大字段曾经静默通过的原因。
pub fn value_bytes(value: Option<&Value>) -> usize {
    match value {
        Some(Value::String(s)) => s.len(),
        Some(Value::Bytes(b)) => b.len(),
        Some(Value::Timestamp(s)) => s.len(),
        Some(Value::Json(v)) => v.to_string().len(),
        Some(Value::Integer(_)) | Some(Value::Float(_)) => 8,
        Some(Value::Bool(_)) => 1,
        Some(Value::Null) | None => 0,
    }
}

/// 一行的字节账。
pub fn row_bytes(row: &[Option<Value>]) -> usize {
    row.iter().map(|v| value_bytes(v.as_ref())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_refuses_an_overrun_without_accounting_it() {
        let mut budget = PipelineBudget::new();
        assert!(budget.try_account(budget.capacity()));
        assert!(!budget.try_account(1), "an overrun must be refused");
        assert_eq!(budget.used(), budget.capacity());
        assert_eq!(budget.max_used(), budget.capacity());
    }

    #[test]
    fn budget_reservation_never_exceeds_capacity() {
        let mut budget = PipelineBudget::new();
        budget.observe_page(3 * 1024 * 1024, 2);
        let reserved = budget.reserve_page(4);
        assert!(
            budget.used() <= budget.capacity(),
            "a reservation may not push the account over capacity"
        );
        budget.release(reserved);
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn affordable_rows_narrow_the_window_but_keep_one_row() {
        let mut budget = PipelineBudget::new();
        assert_eq!(budget.affordable_rows(500), 500);
        budget.observe_page(6 * 1024 * 1024, 2);
        assert_eq!(
            budget.affordable_rows(500),
            1,
            "3 MiB rows leave room for one row per page"
        );
        assert_eq!(budget.affordable_rows(0), 1, "never narrows to zero");
    }

    #[test]
    fn row_estimate_decays_but_follows_the_latest_page() {
        let mut budget = PipelineBudget::new();
        budget.observe_page(4096, 2);
        assert_eq!(budget.row_bytes(), 4096);
        budget.observe_page(16, 2);
        assert_eq!(
            budget.row_bytes(),
            3584,
            "one tiny page cannot erase history"
        );
        budget.observe_page(1_000_000, 2);
        assert_eq!(budget.row_bytes(), 1_000_000, "the latest page wins upward");
    }

    #[test]
    fn value_bytes_charges_the_encoded_size_of_every_value() {
        let payload = serde_json::json!({"payload": [1, 2, 3]});
        let json = Value::Json(payload.clone());
        assert_eq!(
            value_bytes(Some(&json)),
            payload.to_string().len(),
            "JSON must be charged its encoded size"
        );
        let text = "2026-07-01 00:00:00.000+00";
        assert_eq!(
            value_bytes(Some(&Value::Timestamp(text.to_string()))),
            text.len(),
            "a timestamp is charged its text length"
        );
        assert_eq!(value_bytes(Some(&Value::Integer(7))), 8);
        assert_eq!(value_bytes(Some(&Value::Bool(true))), 1);
        assert_eq!(value_bytes(Some(&Value::Null)), 0);
        assert_eq!(value_bytes(None), 0);
    }
}
