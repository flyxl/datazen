//! 目录集成测试的公共脚手架。
//!
//! 这里只有三样东西：**假时钟**、**确定性随机源**、**造数据的快捷函数**。
//!
//! 两条规矩贯穿全部用例：
//!
//! 1. **不真实等待。** 所有时间都来自 [`FakeClock`]，由测试自己推进；目录侧的
//!    `DirectoryClock` 实现收到什么单调纳秒，返回的就是什么，没有一次 `sleep`。
//! 2. **断言非空。** 每条断言都必须能区分「实现正确」与「实现不存在」——
//!    删掉目录实现以后，用例必须失败。空断言（只跑不断言、或断言恒真）在本文件
//!    里不允许出现。

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, EditorSessionId, JobId, OrganizationId, PrincipalId, StageId,
    Timestamp, WorkerId,
};
use datazen_runtime::directory::{
    ClockBase, DirectoryClock, InMemorySessionDirectory, MonoInstant, SessionDraft,
    SessionIdEntropy, SharedClock,
};

/// 假时钟的 UTC 起点：2026-01-01T00:00:00Z 的毫秒数。
pub const T0_UTC_MILLIS: i64 = 1_767_225_600_000;

/// 目录自己的读时钟。单调值由测试推进，投影由 [`ClockBase`] 算出。
///
/// 之所以不能复用 `connection::testing::FakeClock`：那个类型在
/// `#[cfg(any(test, feature = "test-harness"))]` 之下，而集成测试是**外部**
/// crate，拿不到它；本 crate 也没有 `[dev-dependencies]` 可以打开 feature。
pub struct FakeClock {
    base: ClockBase,
    nanos: AtomicU64,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            base: ClockBase::new(MonoInstant::ZERO, T0_UTC_MILLIS),
            nanos: AtomicU64::new(0),
        }
    }

    /// 推进假时钟。**这是唯一的「时间流逝」来源**，用例里不出现真实等待。
    pub fn advance(&self, delta: Duration) {
        let delta_nanos = u64::try_from(delta.as_nanos()).unwrap_or(u64::MAX);
        self.nanos.fetch_add(delta_nanos, Ordering::SeqCst);
    }

    pub fn advance_secs(&self, seconds: u64) {
        self.advance(Duration::from_secs(seconds));
    }

    pub fn set(&self, at: MonoInstant) {
        self.nanos.store(
            u64::try_from(at.as_nanos()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }

    pub fn instant(&self) -> MonoInstant {
        MonoInstant::from_nanos(u128::from(self.nanos.load(Ordering::SeqCst)))
    }

    /// 当前的 UTC 投影（给 `sweep_expired` 之类的调用方墙钟参数用）。
    pub fn utc_now(&self) -> Timestamp {
        self.base.project(self.instant())
    }

    pub fn utc_at(&self, at: MonoInstant) -> Timestamp {
        self.base.project(at)
    }

    pub fn shared(self: &Arc<Self>) -> SharedClock {
        Arc::clone(self) as SharedClock
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryClock for FakeClock {
    fn now(&self) -> MonoInstant {
        self.instant()
    }

    fn project(&self, at: MonoInstant) -> Timestamp {
        self.base.project(at)
    }
}

/// 恒定随机源：每次生成都产出同一串字节，于是「碰撞后重生成」必然再次碰撞。
///
/// 用途是把「重试预算耗尽」这条分支踩实——真实 OS 随机源永远撞不上。
pub struct FixedEntropy {
    byte: u8,
}

impl FixedEntropy {
    pub fn new(byte: u8) -> Self {
        Self { byte }
    }
}

impl SessionIdEntropy for FixedEntropy {
    fn fill(&self, out: &mut [u8]) {
        out.fill(self.byte);
    }
}

/// 循环随机源：第 n 次调用产出第 `n % modulus` 个字节块。
///
/// 用来确定性地制造「先撞一次、第二次换到新 ID」以及「预算耗尽」两种情形。
pub struct CyclicEntropy {
    counter: AtomicU64,
    modulus: u64,
}

impl CyclicEntropy {
    pub fn new(modulus: u64) -> Self {
        assert!(modulus > 0, "循环模数必须为正，否则拿不到不同取值");
        Self {
            counter: AtomicU64::new(0),
            modulus,
        }
    }
}

impl SessionIdEntropy for CyclicEntropy {
    fn fill(&self, out: &mut [u8]) {
        let index = self.counter.fetch_add(1, Ordering::SeqCst) % self.modulus;
        out.fill(u8::try_from(index % 251).unwrap_or(0));
    }
}

/// 造一个「时钟可控 + 随机源可控」的目录。
pub fn directory(
    clock: &Arc<FakeClock>,
    entropy: Arc<dyn SessionIdEntropy>,
) -> InMemorySessionDirectory {
    InMemorySessionDirectory::with_sources(clock.shared(), entropy)
}

/// 常用组合：假时钟 + 循环随机源（默认 64 个互不相同的取值，足够普通用例用）。
pub fn cycling_directory(modulus: u64) -> (Arc<FakeClock>, InMemorySessionDirectory) {
    let clock = Arc::new(FakeClock::new());
    let dir = directory(&clock, Arc::new(CyclicEntropy::new(modulus)));
    (clock, dir)
}

/// 造一个 Editor 归属的会话草稿。目录里**只有**归属与连接 id，没有凭据字段。
pub fn editor_draft(
    organization: &str,
    principal: &str,
    connection: &str,
    client: &str,
) -> SessionDraft {
    SessionDraft::new(
        OrganizationId::new(organization),
        PrincipalId::new(principal),
        ConnectionId::new(connection),
        OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new(client),
            editor_session_id: EditorSessionId::new(format!("{client}-editor")),
        },
        WorkerId::new("worker-1"),
    )
}

/// 造一个 Job 归属的会话草稿。
pub fn job_draft(
    organization: &str,
    principal: &str,
    connection: &str,
    job: &str,
    stage: &str,
) -> SessionDraft {
    SessionDraft::new(
        OrganizationId::new(organization),
        PrincipalId::new(principal),
        ConnectionId::new(connection),
        OwnerRef::Job {
            job_id: JobId::new(job),
            stage_id: StageId::new(stage),
        },
        WorkerId::new("worker-1"),
    )
}

pub fn principal(name: &str) -> PrincipalId {
    PrincipalId::new(name)
}

pub fn client(name: &str) -> ClientInstanceId {
    ClientInstanceId::new(name)
}

pub fn job(name: &str) -> JobId {
    JobId::new(name)
}
