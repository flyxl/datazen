//! `dbSessionId` 与 `runtimeEpoch` 的生成。
//!
//! 两条硬规则：
//!
//! * **没有中央 ID 分配器。** ID 由持有目录的这个进程自己生成，因此每个 worker 都要
//!   能独立起会话，不依赖任何外部序列服务。
//! * **熵不少于 128 位。** [`DB_SESSION_ID_ENTROPY_BITS`] 既是生成常量也是可断言的事实；
//!   碰撞不是「不可能」，而是「检测到就重生成」，重试耗尽必须**失败**而不是返回成功。
//!
//! 熵来源抽成 [`SessionIdEntropy`]，唯一的目的是让 CM-71 能在测试里**强制碰撞**：
//! 生产用 [`OsEntropy`]，测试注入会重复返回同一串字节的脚本源。

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{DbSessionId, RuntimeEpoch, WorkerId};

/// `dbSessionId` 的随机位数下限。契约要求「随机 128 位以上」。
pub const DB_SESSION_ID_ENTROPY_BITS: u32 = 128;

/// 每次生成取用的字节数，与 [`DB_SESSION_ID_ENTROPY_BITS`] 对齐。
pub const ID_ENTROPY_BYTES: usize = (DB_SESSION_ID_ENTROPY_BITS / 8) as usize;

/// 碰撞重生成的上限。超出即失败——**绝不允许**返回一个未经登记的 ID。
pub const MAX_ID_GENERATION_ATTEMPTS: usize = 8;

/// 随机字节来源。
pub trait SessionIdEntropy: Send + Sync + 'static {
    /// 填满 `out`。实现必须是不可预测的；返回全零等于没有熵。
    fn fill(&self, out: &mut [u8]);
}

/// 生产熵源：std 的 `RandomState`。
///
/// 每个进程有一份 128 位的 OS 种子（`RandomState::new()` 每次调用会推进这份种子而不是
/// 重复它），因此每轮拿到的是**同一份 128 位种子下的不同密钥**。文档如实记录：这不是
/// 密码学 DRBG，熵来自操作系统而非本模块；把它换成密码学随机源是调用方的事，
/// 因为目录只要求「≥128 位随机」且**从不落盘**，没有跨进程可预测性面。
#[derive(Debug, Default, Clone, Copy)]
pub struct OsEntropy;

impl SessionIdEntropy for OsEntropy {
    fn fill(&self, out: &mut [u8]) {
        let mut written = 0usize;
        let mut round = 0u64;
        while written < out.len() {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u64(round);
            hasher.write_usize(out.len());
            let word = hasher.finish().to_be_bytes();
            let take = word.len().min(out.len() - written);
            out[written..written + take].copy_from_slice(&word[..take]);
            written += take;
            round = round.saturating_add(1);
        }
    }
}

/// `dbSessionId` 生成器。
pub struct DbSessionIdGenerator {
    entropy: Arc<dyn SessionIdEntropy>,
    attempts: AtomicU64,
}

impl DbSessionIdGenerator {
    pub fn new() -> Self {
        Self::with_entropy(Arc::new(OsEntropy))
    }

    pub fn with_entropy(entropy: Arc<dyn SessionIdEntropy>) -> Self {
        Self {
            entropy,
            attempts: AtomicU64::new(0),
        }
    }

    /// 已尝试生成的次数。只用于观测，测试用它确认「真的重试过」。
    pub fn attempts(&self) -> u64 {
        self.attempts.load(Ordering::Relaxed)
    }

    /// 生成一个候选 ID。**不做去重**——调用方要么拿去登记，要么走
    /// [`DbSessionIdGenerator::generate_distinct`]。
    pub fn generate(&self) -> DbSessionId {
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
        let mut raw = [0u8; ID_ENTROPY_BYTES];
        self.entropy.fill(&mut raw);
        let id = DbSessionId::new(format!("dbs_{}", hex_encode(&raw)));
        tracing::trace!(attempt, "db session id generated");
        id
    }

    /// 生成一个未被占用的 ID。碰撞就重新生成；`max_attempts` 次之后返回 `Err`，
    /// **绝不**返回一个可能被别的 owner 占着的 ID。
    ///
    /// `is_taken` 会在每次尝试时被调用一次，因此它必须自己去拿锁——
    /// 本方法**不持有**任何目录锁。
    pub fn generate_distinct<F>(
        &self,
        is_taken: F,
        max_attempts: usize,
    ) -> Result<DbSessionId, PortError>
    where
        F: Fn(&DbSessionId) -> bool,
    {
        for round in 0..max_attempts {
            let candidate = self.generate();
            if !is_taken(&candidate) {
                return Ok(candidate);
            }
            tracing::warn!(round, "db session id collision; regenerating");
        }
        Err(PortError::BackendUnavailable(format!(
            "db session id still colliding after {max_attempts} attempts"
        )))
    }
}

impl Default for DbSessionIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// `runtimeEpoch` 生成器。
///
/// epoch 的语义是「进程世代」：worker 每次启动换一个新值，于是上一代签发出去的
/// `SessionHandle` 立刻变成显式失效，而不是被悄悄接住。
pub struct RuntimeEpochGenerator {
    entropy: Arc<dyn SessionIdEntropy>,
    attempts: AtomicU64,
}

impl RuntimeEpochGenerator {
    pub fn new() -> Self {
        Self::with_entropy(Arc::new(OsEntropy))
    }

    pub fn with_entropy(entropy: Arc<dyn SessionIdEntropy>) -> Self {
        Self {
            entropy,
            attempts: AtomicU64::new(0),
        }
    }

    pub fn attempts(&self) -> u64 {
        self.attempts.load(Ordering::Relaxed)
    }

    pub fn generate(&self) -> RuntimeEpoch {
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
        let mut raw = [0u8; ID_ENTROPY_BYTES];
        self.entropy.fill(&mut raw);
        let epoch = RuntimeEpoch::new(format!("rte_{}", hex_encode(&raw)));
        tracing::debug!(attempt, %epoch, "runtime epoch generated");
        epoch
    }

    /// worker 启动的唯一入口：每次调用得到一个**新的** epoch。
    ///
    /// 参数留着是因为调用方必须显式说明「哪个 worker 起了这一代」——
    /// 这样 `workerId` 与 epoch 的绑定出现在代码里，而不是靠上下文猜。
    pub fn start_worker_epoch(&self, worker_id: &WorkerId) -> RuntimeEpoch {
        let epoch = self.generate();
        tracing::info!(%worker_id, %epoch, "worker runtime epoch started");
        epoch
    }
}

impl Default for RuntimeEpochGenerator {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}
