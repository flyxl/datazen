//! 物理会话目录：`SessionDirectory` 端口的**单进程**实现。
//!
//! 本模块只回答一个问题——**给定一个 `dbSessionId`，请求应当落到哪个 owner、
//! 哪个 `runtimeEpoch`**。它不知道、也不该知道别的事：
//!
//! * 条目里**只有** `owner` / `runtimeEpoch` / TTL 三样东西，**没有连接、没有凭据**；
//! * **不落盘**：没有快照、没有 append-only 日志、没有墓碑面，进程退出即全丢；
//! * `dbSessionId` 由本模块生成（≥128 位随机），碰撞重生成，**没有中央 ID 分配器**；
//! * 替换提交是原子的：`Prepared` 候选**不可路由**，`Committed` 之后旧条目改关闭路由；
//!   提交结果未知时**保持屏障**，只能按 operation key 查状态来解；
//! * TTL 存服务端**单调时钟**，`expiresAt` 只是最早期限的 UTC 投影，裁决**串行**。
//!
//! 模块分工：
//!
//! | 文件 | 职责 |
//! | --- | --- |
//! | [`id`] | `dbSessionId` / `runtimeEpoch` 的生成与碰撞重生成 |
//! | [`ttl`] | 单调期限集合，最早期限优先 |
//! | [`entry`] | 条目状态机：可路由 / 屏障 / 关闭中 / 关闭 |
//! | [`attachment`] | attachment 令牌签发与归属校验 |
//! | [`commit`] | 替换操作记录与未知结果的屏障 |
//! | [`directory`] | 目录本体与 `SessionDirectory` 端口实现 |
//! | [`lifecycle`] | attach / detach / 执行 / 关闭 / 到期清扫 |
//!
//! 时间一律走 [`DirectoryClock`]，生产用 [`SystemDirectoryClock`]，测试注入假时钟；
//! 任何路径都**不做真实等待**。
//!
//! **调用方的清扫义务**：未定期调用 `SessionDirectory::sweep_expired` 的会话
//! **不会自动过期**。期限到期只写进条目状态，路由闸与挂载闸都**不**做惰性 TTL 判定——
//! `route()` 在期限已过时照样放行。TTL 判定由调用方显式触发（`sweep_expired`），
//! 或经同一 actor 串行仲裁（`InMemorySessionDirectory::adjudicate`）后才生效。
//! 因此把清扫做成周期性任务、放进同一个 actor 的串行循环，是**调用方的义务**，
//! 而不是本模块可以替你兜底的实现细节。

pub mod attachment;
pub mod commit;
pub mod directory;
pub mod entry;
pub mod id;
pub mod lifecycle;
pub mod ttl;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use datazen_platform_api::id::Timestamp;

pub use datazen_platform_api::dto::session::SessionHandle;
/// 契约层没有的 ID 类型在这里补上：**目录自己生成 `dbSessionId`**。
/// 类型本身来自 platform-api（跨 crate 唯一），本模块只负责**生成规则**。
pub use datazen_platform_api::id::{
    AttachmentToken, BlockId, ClientInstanceId, ConnectionId, DbSessionId, EditorSessionId,
    ExecutionId, JobId, OrganizationId, PrincipalId, RuntimeEpoch, StageId, WorkerId,
};
pub use datazen_platform_api::ports::session_directory::{
    CloseDisposition, InvalidationReason, ReplacementCommit, ReplacementOperation,
    ReplacementOutcome, SessionDirectory, SessionOwner,
};

pub use attachment::{
    AttachmentClaim, AttachmentOutcome, AttachmentRejection, AttachmentRequest, TokenDigest,
};
pub use commit::{CommitStatus, RecordState, ReplacementOperationKey, UnknownCommitOutcome};
pub use directory::InMemorySessionDirectory;
pub use entry::{
    Adjudication, ClosureReason, DirectoryEntry, InvalidationRecord, RouteRejection, RoutingState,
    SessionSnapshot,
};
pub use id::{DbSessionIdGenerator, OsEntropy, RuntimeEpochGenerator, SessionIdEntropy};
pub use lifecycle::{ExecutionOccupancy, SessionDraft};
pub use ttl::{Deadline, DeadlineKind, DeadlineSet};

/// 服务端单调时刻：纳秒计数，**不表示任何墙上时间**。
///
/// TTL 的判定只比这个值；`expiresAt` 是投影出去的字符串，墙钟回拨不会让会话提前过期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MonoInstant(u128);

impl MonoInstant {
    pub const ZERO: MonoInstant = MonoInstant(0);

    pub const fn from_nanos(nanos: u128) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u128 {
        self.0
    }

    /// 与更早时刻的间隔。`self` 更早时按 0 处理（不回绕、不 panic）。
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        let nanos = self.0.saturating_sub(earlier.0);
        Duration::new(
            (nanos / 1_000_000_000) as u64,
            (nanos % 1_000_000_000) as u32,
        )
    }

    /// 纳秒数安全收敛为 `i64` 毫秒差，超出范围饱和而不是溢出。
    fn saturating_millis_from(self, earlier: Self) -> i64 {
        let nanos = self.0.saturating_sub(earlier.0);
        let millis = nanos / 1_000_000;
        i64::try_from(millis).unwrap_or(i64::MAX)
    }
}

impl fmt::Display for MonoInstant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ns", self.0)
    }
}

impl std::ops::Add<Duration> for MonoInstant {
    type Output = MonoInstant;

    fn add(self, rhs: Duration) -> MonoInstant {
        MonoInstant(self.0.saturating_add(rhs.as_nanos()))
    }
}

/// 目录读时间的方式。生产实现走系统单调时钟；测试注入假时钟，因此
/// **任何测试都不需要真实等待**。
pub trait DirectoryClock: Send + Sync + 'static {
    /// 当前单调时刻。
    fn now(&self) -> MonoInstant;

    /// 把单调时刻投影成 UTC 字符串。投影**只是**给调用方看的 `expiresAt`，
    /// 判定永远用 [`DirectoryClock::now`] 的单调值。
    fn project(&self, at: MonoInstant) -> Timestamp;
}

/// 单调起点与 UTC 起点的配对。只要提供这一对，「单调 → UTC」的投影就是纯函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockBase {
    origin: MonoInstant,
    origin_millis: i64,
}

impl ClockBase {
    pub const fn new(origin: MonoInstant, origin_millis: i64) -> Self {
        Self {
            origin,
            origin_millis,
        }
    }

    pub const fn origin(&self) -> MonoInstant {
        self.origin
    }

    pub fn project(&self, at: MonoInstant) -> Timestamp {
        project_millis_to_timestamp(self.project_millis(at))
    }

    /// 同一次投影的毫秒数形式。用于「调用方给的墙上时间与服务端投影差多少」这类比较，
    /// 免得调用方把字符串拆开自己算。
    pub fn project_millis(&self, at: MonoInstant) -> i64 {
        self.origin_millis
            .saturating_add(at.saturating_millis_from(self.origin))
    }
}

/// Unix 毫秒 → 规范化 UTC 字符串（`YYYY-MM-DDTHH:MM:SS.mmmZ`）。
///
/// 刻意**定宽且带毫秒**：契约层不解析时刻，`Timestamp` 的 `Ord` 就是字符串序，
/// 所以凡是参与排序的投影必须定宽，秒级精度会把同一秒内的两个期限压成同一个字符串。
/// 这里手写换算而不是引第三方时间库：目录只需要这一个纯函数，且必须逐位可控。
pub fn project_millis_to_timestamp(millis: i64) -> Timestamp {
    let seconds = millis.div_euclid(1_000);
    let sub_millis = millis.rem_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    Timestamp::new(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{sub_millis:03}Z"
    ))
}

/// 天数（Unix 纪元起）→ 公历年月日。Howard Hinnant 的 civil_from_days，负数年份亦成立。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// 公历年月日 → 天数（Unix 纪元起）。[`civil_from_days`] 的逆运算。
///
/// 只用来读回本模块自己写出去的投影串（比如比较调用方给的 `now` 与服务端投影差多少），
/// 所以对非法输入一律返回 `None`，绝不用「猜一个大概」。
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    debug_assert!((1..=12).contains(&month), "month out of range");
    debug_assert!((1..=31).contains(&day), "day out of range");
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = (shifted_year - era * 400) as u64;
    let month_position = if month > 2 { month - 3 } else { month + 9 } as u64;
    let day_of_year = (153 * month_position + 2) / 5 + u64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era as i64 - 719_468
}

/// 生产时钟：`Instant` 测经过时间，`SystemTime` 只用来钉住一个 UTC 起点。
#[derive(Debug)]
pub struct SystemDirectoryClock {
    base: ClockBase,
    origin: std::time::Instant,
}

impl SystemDirectoryClock {
    pub fn new() -> Self {
        let origin = std::time::Instant::now();
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        Self {
            base: ClockBase::new(MonoInstant::ZERO, millis),
            origin,
        }
    }

    /// 以给定的单调起点 / UTC 起点建表，便于复现投影。
    pub fn from_parts(base: ClockBase, origin: std::time::Instant) -> Self {
        Self { base, origin }
    }

    /// 投影基准，供需要自行推算投影的调用方（例如注册期给出初始 `last_business_activity`）。
    pub fn base(&self) -> ClockBase {
        self.base
    }
}

impl Default for SystemDirectoryClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryClock for SystemDirectoryClock {
    fn now(&self) -> MonoInstant {
        MonoInstant::from_nanos(self.origin.elapsed().as_nanos())
    }

    fn project(&self, at: MonoInstant) -> Timestamp {
        self.base.project(at)
    }
}

/// 目录实例共享的时钟句柄类型别名。
pub type SharedClock = Arc<dyn DirectoryClock>;
