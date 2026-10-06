//! 记录保留期与令牌授予登记。
//!
//! # 保留期存在的理由
//!
//! 幂等记录不能永久留着，但删早了会**把洞打开**。逐一遍一遍：
//!
//! 1. 令牌 T 在 `E`（到期）之后又过了 `R = E + 24h`；
//! 2. 清扫把 T 的授予与账本记录一起删掉；
//! 3. 客户端在时刻 `t` 重放 T。
//!
//! 若 `t >= E`，第 0 步令牌闸门按 T **自己签发的 `expires_at`** 拒掉它，请求根本走不到
//! 账本——「删除后仍被拒绝」成立。
//!
//! 但若 `t < E`，令牌签名完好、尚未过期，会一路放行到账本；记录已经删干净，
//! 查重得到 `Miss`，于是**真的再执行一遍**。这件事被明令禁止。
//!
//! 所以不变量是：**`retained_until >= expires_at`**。只有「令牌先按自己的签名过期，
//! 之后才允许删授予」成立，上面第 3 步才落在 `t >= E` 那一侧。
//!
//! # 这条不变量在代码里的形状
//!
//! **不是注释里的约定，是字段本身不可被违反。** [`Grant`] 的 `retained_until` 是私有字段，
//! 唯一构造函数 [`Grant::for_execution`] 恒等于 `expires_at + RETENTION_AFTER_EXPIRY_NANOS`，
//! 生产路径**没有**任何入口能写出一个 `retained_until < expires_at` 的授予。
//!
//! 光靠"构造不了"还不够——清扫处**再显式判一次**两道闸（`now >= expires_at` 且
//! `now >= retained_until`），缺一即列入 [`SweepReport::refused`] 并 `tracing::warn!`。
//! 正常数据下 `refused` 恒空；一旦有人日后加了别的人口子把它填满，警告会先响起来。
//!
//! # 提前删除怎么办：退役墓碑
//!
//! 提前删除这个洞是**堵死**的，不是靠约定：
//!
//! - [`GrantRegistry::sweep`] 是生产路径，它**根本不允许**删未过期的授予；
//! - 万一有人绕过清扫直接删（宿主侧运维、将来的真后端），删掉的同时留下**退役墓碑**；
//!   [`super::token::SubmissionTokenGuard::admit`] 第 0 步先查墓碑，所以
//!   「令牌仍在有效期内但授予已被删除 → 重放」被 [`TokenRejection::Tombstoned`] 拒掉。
//!
//! 墓碑只存摘要与到期时刻，长度有界：一旦 `now >= expires_at_nanos`，令牌本身就会被
//! `Expired` 拒掉，墓碑使命完成，可以清掉。墓碑**不会**无限增长，也不会因为清掉
//! 而重新打开洞。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::connection::ExecutionId;
use crate::gateway::idempotency::IdempotencyScope;

/// 到期之后仍须保留的时长（24 小时虚拟时间）。
pub const RETENTION_AFTER_EXPIRY_NANOS: u64 = 24 * 60 * 60 * 1_000_000_000;

/// 一条授予：令牌第一次被受理时登记，与幂等账本里的记录通过 `executionId` 关联。
///
/// 之所以与 [`crate::gateway::idempotency::IdempotencyRecord`] **分开存**：
/// 那条记录是被既有契约测试逐字段构造的冻结结构，加字段必破它。保留期信息属于令牌层，
/// 塞进幂等记录的结构等于让一张别人的表替令牌层保存状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    execution_id: ExecutionId,
    /// 账本作用域。保留它是因为登记表以**令牌摘要**为键，而账本以作用域为键，
    /// 清扫时需要靠它把两边对上——只记摘要就永远删不掉账本里那条记录，
    /// 「删除记录后重放不再执行」会退化成「记录还在，只是令牌先过期了」的假断言。
    scope: IdempotencyScope,
    expires_at_nanos: u64,
    retained_until_nanos: u64,
}

impl Grant {
    /// 生产构造。`retained_until` **恒由 `expires_at` 算出**，不是入参——
    /// 这就是 `retained_until >= expires_at` 成为结构不变量而非口头约定的形状。
    pub fn for_execution(
        execution_id: ExecutionId,
        scope: IdempotencyScope,
        _issued_at_nanos: u64,
        expires_at_nanos: u64,
    ) -> Self {
        Self {
            execution_id,
            scope,
            expires_at_nanos,
            retained_until_nanos: expires_at_nanos.saturating_add(RETENTION_AFTER_EXPIRY_NANOS),
        }
    }

    /// 构造一条**违反** `retained_until >= expires_at` 的授予。
    ///
    /// 只为让测试能把那个状态**造出来**——不变量本身在生产路径上不可违反，
    /// 正因为不可违反，才需要一个显式的、只在测试期存在的口子来证明清扫处那道
    /// 显式判定真的在拦。`#[cfg]` 之外不存在这个方法。
    #[cfg(any(test, feature = "test-harness"))]
    pub fn with_retained_until_for_test(
        execution_id: ExecutionId,
        scope: IdempotencyScope,
        expires_at_nanos: u64,
        retained_until_nanos: u64,
    ) -> Self {
        Self {
            execution_id,
            scope,
            expires_at_nanos,
            retained_until_nanos,
        }
    }

    pub fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    pub fn scope(&self) -> &IdempotencyScope {
        &self.scope
    }

    pub fn expires_at_nanos(&self) -> u64 {
        self.expires_at_nanos
    }

    pub fn retained_until_nanos(&self) -> u64 {
        self.retained_until_nanos
    }

    /// 令牌自身是否已过期（只看令牌签发的到期时刻，不看保留期）。
    pub fn is_expired_at(&self, now_nanos: u64) -> bool {
        now_nanos >= self.expires_at_nanos
    }
}

/// 退役墓碑。授予被删之后仍在闸门第 0 步挡住重放。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Retirement {
    /// 令牌的到期时刻。墓碑只需活到这一刻——之后 `Expired` 接手。
    expires_at_nanos: u64,
}

/// 一条被退役的授予。带着账本作用域，好让调用方把账本里那条记录**一并**删掉。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredGrant {
    pub digest: String,
    pub scope: IdempotencyScope,
    pub execution_id: ExecutionId,
}

/// 清扫结果。`refused` 是**不变量被绕过时的报警**，正常数据下恒空。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub deleted: Vec<RetiredGrant>,
    pub refused: Vec<String>,
    /// 过期后使命完成、可以安全清掉的墓碑数量。
    pub tombstones_pruned: usize,
}

impl SweepReport {
    pub fn is_empty(&self) -> bool {
        self.deleted.is_empty() && self.refused.is_empty() && self.tombstones_pruned == 0
    }
}

/// 一次保留期清扫在网关层的最终结果。
///
/// `retired` 与 `ledger_deleted` 应当相等；不等说明 store 不支持删除
/// （默认实现会返回 `deleteUnsupported`），此时授予已退役、墓碑已立，
/// 重放仍被拒，但记录还留着，需要运维留意。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionSweep {
    pub retired: usize,
    pub ledger_deleted: usize,
    pub refused: Vec<String>,
    pub tombstones_pruned: usize,
}

#[derive(Debug, Default)]
struct RegistryInner {
    grants: HashMap<String, Grant>,
    retirements: HashMap<String, Retirement>,
}

/// 授予登记表（进程内）。
///
/// 单进程网关用它就够了，与 [`crate::gateway::idempotency::InMemoryIdempotencyStore`]
/// 同一条生命周期。需要跨进程共享的部署形态应替换实现，而不是在本类型上叠锁。
#[derive(Debug, Default)]
pub struct GrantRegistry {
    inner: Mutex<RegistryInner>,
}

impl GrantRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 登记一条授予。同一摘要重复登记取后一次——与「重复写是调用方 bug」
    /// 不同，这里允许重登记是因为签发方可能在重试中重新登记同一张令牌。
    pub fn record(&self, digest: impl Into<String>, grant: Grant) {
        let digest = digest.into();
        // 锁中毒意味着已经有别的线程在 panic。如实放行：登记失败的后果是这道令牌
        // 少了条授予，重放路径会退化成查不到记录而拒绝，方向仍然是 fail-closed。
        if let Ok(mut guard) = self.inner.lock() {
            guard.grants.insert(digest.clone(), grant);
            guard.retirements.remove(&digest);
        }
    }

    pub fn grant(&self, digest: &str) -> Option<Grant> {
        match self.inner.lock() {
            Ok(guard) => guard.grants.get(digest).cloned(),
            Err(_) => None,
        }
    }

    pub fn is_retired(&self, digest: &str) -> bool {
        match self.inner.lock() {
            Ok(guard) => guard.retirements.contains_key(digest),
            // 读不出来时按「已退役」处理：拒绝比放行安全。
            Err(_) => true,
        }
    }

    pub fn len(&self) -> usize {
        self.count(|inner| inner.grants.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn count(&self, f: impl FnOnce(&RegistryInner) -> usize) -> usize {
        match self.inner.lock() {
            Ok(guard) => f(&guard),
            Err(_) => 0,
        }
    }

    /// 保留期清扫。
    ///
    /// 删除一道闸也不放过：**必须同时** `now >= expires_at`（令牌已过期）
    /// **且** `now >= retained_until`（保留期已过）。后者在不变量下是冗余的，
    /// 但它必须**显式存在**——否则哪天有人加了别的人口子绕过构造时，
    /// 就没有第二道闸在拦了。
    ///
    /// 剪枝**先于**删除跑，顺序同样是有意的：删除动作会在同一趟里写下新墓碑，
    /// 若剪枝排在后面，这趟刚立的墓碑会立刻被自己的 `now` 剪掉。墓碑存在的意义
    /// 正是「授予已不在、令牌还可能没过期」那段窗口，先删后剪等于把窗口关死。
    pub fn sweep(&self, now_nanos: u64) -> SweepReport {
        let mut report = SweepReport::default();
        if let Ok(mut guard) = self.inner.lock() {
            let before = guard.retirements.len();
            guard
                .retirements
                .retain(|_, retirement| now_nanos < retirement.expires_at_nanos);
            report.tombstones_pruned = before - guard.retirements.len();

            let expired: Vec<String> = guard
                .grants
                .iter()
                .filter(|(_, grant)| {
                    grant.is_expired_at(now_nanos) && now_nanos >= grant.retained_until_nanos
                })
                .map(|(digest, _)| digest.clone())
                .collect();
            for digest in expired {
                if let Some(grant) = guard.grants.remove(&digest) {
                    // 删授予的同时留墓碑：即便删除被提前调用，重放也仍被拒。
                    guard.retirements.insert(
                        digest.clone(),
                        Retirement {
                            expires_at_nanos: grant.expires_at_nanos,
                        },
                    );
                    report.deleted.push(RetiredGrant {
                        digest,
                        scope: grant.scope.clone(),
                        execution_id: grant.execution_id,
                    });
                }
            }
            let refused: Vec<String> = guard
                .grants
                .iter()
                .filter(|(_, grant)| {
                    now_nanos >= grant.retained_until_nanos && !grant.is_expired_at(now_nanos)
                })
                .map(|(digest, _)| digest.clone())
                .collect();
            for digest in &refused {
                tracing::warn!(
                    digest = %digest,
                    now_nanos,
                    "retained_until 已到而 expires_at 未到：违反 retained_until >= expires_at，拒绝删除"
                );
            }
            report.refused = refused;
        }
        report
    }

    /// 强制退役一条授予，**不做任何到期判定**。
    ///
    /// 这是「有人把 delete 提前调用了」那条路径。生产构建里不存在本方法——
    /// 所以那个洞在生产里**不可构造**；它存在只是为了让测试能把该状态造出来，
    /// 并证明闸门第 0 步的墓碑判定仍会拒绝重放。
    #[cfg(any(test, feature = "test-harness"))]
    pub fn force_retire(&self, digest: &str, now_nanos: u64) -> bool {
        match self.inner.lock() {
            Ok(mut guard) => {
                let Some(grant) = guard.grants.remove(digest) else {
                    return false;
                };
                guard.retirements.insert(
                    digest.to_owned(),
                    Retirement {
                        expires_at_nanos: grant.expires_at_nanos,
                    },
                );
                let _ = now_nanos;
                true
            }
            Err(_) => false,
        }
    }

    /// 故意在持锁时 panic，把锁弄成中毒态。
    ///
    /// 只为让「锁中毒时 `is_retired` 仍回答『已退役』」这条 fail-closed 行为
    /// 可被测试。`#[cfg(test)]` 之外不存在本方法，生产构建里那个洞也就不可构造。
    #[cfg(test)]
    pub fn poison_for_test(&self) {
        super::testing_support::poison_lock(&self.inner);
    }
}
