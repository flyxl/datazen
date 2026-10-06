//! §6.4 空闲驱逐：把「到期」这件事在**注入的时钟**上做实，并且对每一种非成功
//! 答复分别记账。
//!
//! 从 `registry.rs` 拆出来的理由：`evict_idle_at` 的**账目表**（三种非成功情形各自
//! 是否摘表、是否进返回值）比它的控制流重要得多，而表长大到足以把 `registry.rs`
//! 顶过单文件上限。留在原处时，它只是 900 行文件里一段看起来像普通循环的代码。
//!
//! 拆出去之后这里能整段读，不依赖 `registry.rs` 的其余上下文——这正是它需要的
//! 待遇：这一段的行为要靠「四种结局分别是什么」来评审，而不是靠「它在哪个函数里」。
//!
//! 私有可见性不变：子模块能看见父模块的私有项，所以 `forget`、`write_table`、
//! `quota` 都照旧直接用，不新增任何 `pub(super)` 出口。

use crate::connection::{RuntimeError, SessionView};
use crate::registry::actor::ExecCommand;
use crate::registry::registry::SessionRegistry;

impl SessionRegistry {
    /// §6.4：按注入的绝对毫秒驱逐到期的空闲会话。
    ///
    /// 时间由调用方注入：本模块**不读** `Instant::now()`，到期判定因此在测试里
    /// 完全确定，不依赖真实时钟。
    ///
    /// # 一次驱逐答复的三种非成功情形是三件事，不是一件事
    ///
    /// 这里曾经写作 `let Ok(Some(view)) = … else { continue }`：把所有非成功情形压成
    /// 同一个「什么都不做」。它们对**账**的影响完全不同，所以逐向判别：
    ///
    /// | actor 答复 | 事实 | 摘表还额度？ | 进返回值？ |
    /// | --- | --- | --- | --- |
    /// | `Ok(Some(view))` | 到期，§9.4 四步干净跑完 | 是 | 是 |
    /// | `Ok(None)` | 没有物理资源 / 未设期限 / 未到期限 | **否** | 否 |
    /// | `Err(CloseRejected(_))` | 闸门在**下发之前**拦下（§7.4-6 替换屏障），什么都没发生 | **否** | 否 |
    /// | `Err(SessionLost(_))` | §9.4 四步**跑完之后**才写出来的：关闭已发出、`physical` 已清空 | 是 | 否 |
    /// | 其余 `Err(_)` | **没送达 / 送达不明**（发送失败，或 actor 任务在 §9.4 的某个 await 中途没了） | **否** | 否 |
    ///
    /// 每一格的行为后果，缺任何一格都会踩到：
    ///
    /// - 把 `Ok(None)` 并进「已丢失」⇒ 未到期的会话被当成已关而摘掉、额度白还一格，
    ///   且 `session_view` 从此查不到一个还在服务的会话。这正是「只补后两路」会连带点红
    ///   `未设空闲期限的会话永不被驱逐`、`空闲到期才驱逐_未到期一律不动手` 两条既有用例的成因。
    /// - 把 `Err(CloseRejected(_))` 并进「已丢失」⇒ 一次**派发前**被拒的驱逐会凭空造出
    ///   一格额度：物理资源与已登记句柄原封不动，表项却没了，随后替换编排第 ⑥ 步的
    ///   `close_registered` 定位不到旧会话，**旧物理资源再也没人去关**。
    /// - 把 `Err(SessionLost(_))` 并进「什么都没发生」⇒ 留下一个「物理资源已死、行还在表里、
    ///   额度还扣着」的僵尸会话：`session_view` 仍查得到它、仍能向一个已经不存在的资源发执行，
    ///   而那一格额度永久卡死，没有任何后续路径会想起它。这与
    ///   [`SessionRegistry::close_registered`] 里 R-01 治的是同一种病，处置也必须一致：
    ///   [`crate::registry::actor::release`] **只在四步全部跑完之后**才写出 `SessionLost`
    ///   （句柄已按批次发过终结、`state.physical` 已清空、绑定已作废），所以这条答复证明的是
    ///   「关闭已经发出过」，不可判定的只是某个句柄的命运，不是「有没有关掉」。
    /// - 把「其余 `Err`」并进「已丢失」⇒ 反向的错：物理资源可能仍活着，登记表却把它静默记成
    ///   已作废并把额度发了出去。这一格与 [`SessionRegistry::invalidate_worker`] 的三态表
    ///   同源，处置也必须一致：保留表项、留下可查的痕迹，等显式处置。
    ///
    /// `Err(SessionLost(_))` 这一格**不**带回视图：actor 给出的 `SessionView` 写着
    /// `Lost` / `Closing`，交回调用方等于替一个不可判定的会话报「已驱逐」。摘行与还额度走
    /// 既有的 [`SessionRegistry::forget`]——它对**已经**被别的路径摘掉的行不会二次归还额度。
    pub async fn evict_idle_at(&self, at_ms: u64) -> Vec<SessionView> {
        self.audit.pump();
        let targets = self.read_table().records();
        let mut evicted = Vec::new();
        for record in targets {
            match record
                .actor
                .exec(|reply| ExecCommand::Evict { at_ms, reply })
                .await
            {
                // 到期且干净跑完：摘行、还额度、把视图交回调用方。
                Ok(Some(view)) => {
                    self.forget(&record.db_session_id);
                    evicted.push(view);
                }
                // 未到期 / 没有物理资源 / 未设期限：一次正常的空操作，不动任何东西。
                Ok(None) => {}
                // 闸门在下发前拦下（§7.4-6 替换屏障）：什么都没发生，表与额度都留着。
                Err(RuntimeError::CloseRejected(_)) => {}
                // §9.4 已跑完、关闭已发出：按「已丢失」摘表还额度，细节留在 actor 侧墓碑与审计。
                Err(RuntimeError::SessionLost(reason)) => {
                    tracing::warn!(
                        db_session_id = record.db_session_id.as_str(),
                        %reason,
                        "空闲驱逐未得干净终态：§9.4 释放例程已跑完、物理资源已发出关闭，\
                         摘表归还额度；不可判定细节留在 actor 侧的墓碑与审计里"
                    );
                    self.forget(&record.db_session_id);
                }
                // 没送达 / 送达不明：物理资源可能仍活着，不得把它静默记成已作废。
                Err(error) => {
                    tracing::warn!(
                        db_session_id = record.db_session_id.as_str(),
                        reason = error.reason(),
                        "空闲驱逐的请求没有送达或送达不明（actor 任务可能在 §9.4 的 await 中途没了）：\
                         物理资源可能仍活着，保留表项与额度，等待显式处置"
                    );
                }
            }
        }
        evicted
    }
}
