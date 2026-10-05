//! CM-60 资源压力 / drain 门禁的夹具。
//!
//! 三条纪律（与 `tests/support` 同口径）：
//!
//! 1. **没有真实等待**：账本入口每个都显式收 `now_ms`，账本本身不含时间源，
//!    所以整条压力链是同步的——没有 `sleep`、没有定时器竞态、没有偶发失败。
//! 2. **顺序只来自 `seq`**：变化点由单一原子计数器定序，不靠时间推断先后。
//! 3. **不变量在变化点上当场判**：不是收尾时看一眼总数，见 [`journal`]。

mod journal;
mod scenario;

pub use journal::Change;
pub use scenario::{run, OPERATIONS, REQUEST_MS, RESERVED, SESSIONS_PER_USER, TICKS, TOTAL, USERS};
