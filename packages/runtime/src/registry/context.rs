//! §7.4 `setSessionContext` 的运行时契约层：**候选资源 + §12 原子发布**。
//!
//! 这一层此前不存在：`setSessionContext` 在 `platform-api` / `application` /
//! `backend-client` 三处都有契约与调用方，唯独 runtime 侧没有执行者。补它不能靠
//! 造一个同名入口糊住判据——那只是把一个洞改名。真正缺的是**次序**：把 §12 已经
//! 建好的替换状态机真正驱动起来。
//!
//! ## 次序（§7.4-6 的十项在这里逐条落点）
//!
//! ```text
//!   定位旧会话 ─▶ 上下文修订校验 ─▶ 幂等短路 ─▶ 额度预检
//!        │
//!        ▼  ① 候选物理资源（open_candidate，不进可见表 ⇒ 不可见、不可定位、不可执行）
//!        ▼  ②③④ 旧 actor 举发放闸门（停止发放新执行；句柄与资源原封不动）
//!        ▼  §12 Prepared：候选 id 被占住，候选不可路由
//!        ▼  §12 Committed：原子切换，恰好一边可路由
//!        ▼  ⑥ 旧会话走**唯一**的 §9.4 释放例程（close_registered）
//!        ▼  publish_candidate：候选此刻才进可见表
//!        ▼  签发新会话自己的 attachment 令牌 ⇒ 挂载 ⇒ 回执
//! ```
//!
//! 三个设计决定值得写下来，因为它们正是 CM-74 在这条路径上成立的原因：
//!
//! - **第 ⑥ 步不重写释放逻辑。** 它调用既有的 `close_registered` → `ExecCommand::Close`
//!   → `actor::release`，也就是 §9.4 唯一那条四步例程。CM-74 要求的「句柄在物理
//!   资源关闭前已在**原 resource** 上终结并从 actor 注销」因此是**构造性**成立的：
//!   替换路径没有第二条关闭路径可以漂移。
//! - **闸门不碰句柄。** 闸门只停止「发新的」，收「旧的」一律归 §9.4。两者混写就会
//!   长出第二条释放路径，而第二条路径一定和第一条漂移。
//! - **回执里的令牌是**新会话自己**签发的。** 候选不经 `open_session`（那条路在登记
//!   的同一刻就把令牌交出去了），§12 的提交也不签发；不补这一下，§7.4 回执的
//!   `attachmentToken` 就是个谁也用不上的字段。
//!
//! ## 幂等是 §12 原生的，不是外挂的
//!
//! `ReplacementOperationKey::for_handle` 由**旧句柄**确定性地派生，重试同一个旧句柄
//! 必然落到同一个 key 上，`commit_status` 于是直接给出 `Committed { handle }`。
//! 同一份回执被恢复，候选**永不重建**。这里不去碰 `gateway/idempotency.rs`（CM-70
//! 在飞），幂等性完全由目录承担。
//!
//! 幂等短路排在**所有**旧会话检查**之前**，这是它必须待的位置：替换一旦提交，旧会话
//! 就注销了，先定位就会先撞上 `UnknownSession`，重放永远走不到短路。
//!
//! 恢复的是**同一份回执的同一枚令牌**。这不是取舍，是规格写死的：§7.4 `:566` 要求
//! 「网络丢失后**同键重试返回原 receipt**」，§13.1 `:800` 把它拆开说——回执里的
//! SessionView / attachmentToken / receipt「只在 owner 内存保存至令牌过期或 runtime 终止」，
//! 于是「响应丢失时，在原 runtime 内**同键返回同 session/token/候选提交结果**」。
//!
//! 「只在 owner 内存」是**留在内存里**的意思，不是「不留」。所以 `ContextReplacer` 自带
//! 一张 `operation key → 已签发令牌` 的表：首次签发时记下，重试时原样发回。目录侧仍只
//! 留摘要（`:799`：durable 记录只存请求摘要与 receipt 的 durable 投影）——**目录不需要
//! 留令牌原文，owner 需要**，这两件事不矛盾。
//!
//! 为什么必须这样，而不是每次重签一枚：重签会把目录里的摘要**换掉**，上一枚令牌随即
//! 作废。若响应是在网络层丢的，调用方手上**没有**那一枚新令牌，它只有最初那枚——于是
//! 一次纯粹的超时会把「替换已提交」变成「调用方永远附着不上新会话」，而重试本身还是
//! 唯一的出路。这正是 `:800` 把「同键返回同 token」写进来的原因：幂等的对象是**回执**，
//! 不是回执里某个可以被随时换新的字段。
//!
//! （上一版本这里写的是「重签是设计选择，不是缺陷」，理由是「目录只留摘要」。那句话把
//! 一处可修的开口提升成了永久断言，且前提读错了：`:799` 限制的是 **durable 记录**，
//! `:800` 紧接着就把这些对象安置在 owner 内存里。目录存不存原文，与 owner 能不能把
//! 同一枚令牌再发一次，是两回事。）
//!
//! ## 两种句柄
//!
//! runtime 内部用 `connection::SessionHandle`（`dbSessionId` + `Counter`），
//! 目录用 `dto::session::SessionHandle`（`dbSessionId` + 字符串 `RuntimeEpoch`）。
//! 二者是既有事实，本模块在边界上做一次**确定性**换算：operation key 由前者派生，
//! 因此重试必然命中同一个 key。
//!
//! ## 失败语义
//!
//! - **提交前失败**：候选销毁（同样走 §9.4）、闸门放下、旧会话**原样**继续。
//!   提交前后是唯一的分界线——分界线之前什么都没变。
//! - **提交后失败**：只允许恢复同一份回执。新会话已经是既成事实，替换不得倒退。
//!
//! ## 调用方契约（接线时必须知道的四件事）
//!
//! `ContextReplacer::replace` 目前**没有生产调用方**，整条编排由测试驱动。接线时下面
//! 四条是前提，代码里读不出别的默认值：
//!
//! 1. **`candidate_db_session_id` 必须唯一。** 撞上在册行会让 `publish_candidate` 失败，
//!    而失败发生在 §12 `Committed` **之后**：目录已提交、旧会话已注销、候选却从未进表。
//!    这一格的额度在 `publish_candidate` 的失败分支里显式退还（`refund_candidate`），
//!    所以不泄漏；但**调用方拿不到可用的新会话**，重试也不会变好。撞号要靠调用方在
//!    分配 id 时避开，不是靠重试。
//! 2. **上述失败态不是静默的。** 幂等短路会核对候选在注册表里在不在：不在就报
//!    `InvariantBroken("committedWithoutHostEntry")`，**不发**回执。发出去就等于把一枚
//!    没有宿主入口的句柄交给调用方去附着，失败点会被推迟到很远的地方。
//! 3. **重试的回执可以整份缓存。** 同一旧句柄的重试返回**同一个** `session` 句柄、
//!    **同一枚** `attachment_token`、同一份被替换关系（§7.4 `:566` + §13.1 `:800`）。
//!    调用方缓存任意一次响应都是安全的；**不要**去比 `attachment_token` 是否变了再决定
//!    用哪一枚——真要比，先确认自己没把「重签」当成规格。
//!    这枚令牌在 `ContextReplacer` 的 owner 内存里，与 `ContextReplacer` 同生共死；
//!    它不会随 `ContextReplacer` 一起持久化，所以跨进程、跨 runtime 的重放必须重新发起
//!    一次 `setSessionContext`（`:800` 的留存期限就是「令牌过期或 runtime 终止」）。
//! 4. **`expected_context_revision` 是乐观并发闸门，不是提示。** 传错一律
//!    `ContextRevisionMismatch`，且**旧会话一点不动**（额度不变、目录不提交）。
//!    §4.4 `:392`「`configRevision/contextRevision` 用于版本与上下文冲突」、§7.1 `:526`
//!    「后续排队请求仍要重新校验 `contextRevision`」就落在这一个比较上，不能改成
//!    「差得不多就算了」。
//!
//! ## 「提交替换 × 并发空闲驱逐」这个交错的可达性
//!
//! 替换提交（`publish_candidate`）与并发驱逐之间的窗口，历来被读成一句话：
//! 「旧会话的句柄会不会被一次并发驱逐终结到**新** resource 上，从而把旧事务提交进新会话」。
//! 逐段核对之后，**这一半结构上不可达**，理由是三条各自独立、且都能从代码读出来的事实：
//!
//! 1. **终结与关闭的资源归属由 actor 自己决定，不由调用方决定。**
//!    [`crate::registry::actor::release`] 的 `close` 用的是 `state.physical`（该 actor
//!    自己那份），而 `finalize_handles` 用的是**每个句柄登记时**记的 `resource_id`。
//!    候选是**另一个 actor**（`open_candidate` 起的），它的 `state.physical` 是新资源。
//!    两个 actor 各有一本自己的句柄账。所以「把旧句柄发到新资源上」需要旧句柄出现在
//!    候选的账上，而 [`crate::registry::handles::HandleRegistry::register`] 只被
//!    `exec::apply_completion` 在**同一个 actor 内**调用——没有任何一条路径把 A 的句柄
//!    搬进 B 的账本。驱逐按 `dbSessionId` 定位 actor，它连「哪一个 actor」都选不错。
//! 2. **旧会话在候选进表之前就已经注销。** 编排顺序是
//!    ⑥hold → ⑦Prepared/Committed → ⑧`close_registered(旧)` → ⑨`publish_candidate`。
//!    第 ⑨ 步之前旧行已被 `forget` 摘掉，此后 `evict_idle_at` 遍历不到它；
//!    第 ⑧ 步走的是**唯一**那条 §9.4 释放例程，终结与关闭都落在旧资源上——
//!    即「落在旧 resource」这一支如实成立。
//! 3. **候选自己没有空闲期限。** `open_request` 给候选的 `OpenRequest` 把
//!    `idle_deadline_ms` 置成 `None`，于是 actor 侧 `evict_idle` 对它回 `Ok(None)`，
//!    而 `Ok(None)` 在登记表侧是一次**不动任何东西**的空操作。候选发布之后就是普通
//!    在册会话，这条是它不被并发驱逐的唯一支点，所以它有独立用例
//!    （`tests/registry_evict_replacement.rs` 的
//!    `发布后的候选没有空闲期限_驱逐对它是不动`）：把 `None` 改成 `Some(..)`，
//!    那条用例立刻红，上面这句论断同时失效。
//!
//! **可达的那一半不是「终结到新资源」，而是驱逐的答复怎么记账。** 屏障期间
//! （⑥ 之后、⑧ 之前）到达的 `Evict` 被发放闸门拒成
//! `CloseRejected("replacementInProgress")`（见 [`crate::registry::actor::context::gate`]）。
//! 这是一次**派发前**的拒绝：物理资源、已登记句柄、期限原封不动。登记表此时若把它
//! 当成「§9.4 已跑完、资源已关」去摘行还额度，后果是具体的——第 ⑧ 步的
//! `close_registered` 定位不到旧会话，于是**旧物理资源连同它的全部已登记句柄再也没有
//! 人处置**：既不落在旧 resource，也不落在新 resource，而是悬空。这正是判据
//! 「或明确失败」要排除的形状。该格由
//! `tests/registry_evict_replacement.rs` 的
//! `屏障期间到来的驱逐不得摘行_旧句柄仍由替换例程在原资源终结` 钉住（探针站在
//! `Prepared` 提交那一刻，即屏障内部；不靠时间推进制造窗口）。
//!
//! 为什么这里写结论而不是留一个用例：可达的部分已有用例，不可达的部分**构造不出来**。
//! 为一个构造不出来的交错写一个「它没发生」的用例，等于写一条无论实现对错都绿的断言——
//! 那是位置相关的假覆盖。所以不可达的结论留在代码旁，且它的三个支点分别有出处：
//! 支点 1 与 2 是本文件与 `release`/`handles` 里可直接读到的代码事实，支点 3 有反证用例。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, DbSessionId, PrincipalId, RuntimeEpoch as PlatformEpoch,
};
use datazen_platform_api::ports::session_directory::{
    ReplacementCommit, ReplacementOperation, ReplacementOutcome, SessionDirectory, SessionOwner,
};
use tracing::warn;

use crate::connection::{CloseMode, ExecutionTarget, RuntimeError, SessionHandle, SessionView};
use crate::directory::{
    AttachmentRejection, CommitStatus, InMemorySessionDirectory, ReplacementOperationKey,
    SessionHandle as DirectoryHandle,
};
use crate::registry::actor::OpenRequest;
use crate::registry::epoch::epoch_string;
use crate::registry::port::SessionPort;
use crate::registry::{SessionActor, SessionRegistry};

/// §7.4 编排需要的目录能力。
///
/// 之所以在 runtime 侧另立一个窄端口，而不是往 platform-api 的 `SessionDirectory`
/// 上加方法：§12 的 `CommitStatus` 与 `ReplacementOperationKey` 是 runtime 的类型，
/// 把它们搬进契约层等于让契约层反过来依赖运行时表示。加端口比搬类型便宜，
/// 而且这一层需要的能力就是下面这五个，多一个都不给：编排全程向目录要的，
/// 一项不多、一项不少。
#[async_trait]
pub trait ReplacementDirectory: Send + Sync + 'static {
    /// 取旧会话的 owner。`None` = 目录里没有这个条目。
    async fn owner_of(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError>;

    /// 按 operation key 查替换的落定状态（§7.4 幂等的依据）。
    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus;

    /// §12 的 `Prepared` / `Committed` / `RolledBack` 原子提交。
    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError>;

    /// 给**已发布**的新会话签发 attachment 令牌。§7.4 回执里的那个字段。
    ///
    /// 之所以是端口方法而不是让调用方自带：候选不经 `open_session`，
    /// 提交协议也不签发，调用方手里根本不存在一枚对新会话有效的令牌。
    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, AttachmentRejection>;

    /// 在新会话上挂载，验的是**新会话自己签发**的那枚令牌。
    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), AttachmentRejection>;
}

#[async_trait]
impl ReplacementDirectory for InMemorySessionDirectory {
    async fn owner_of(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        SessionDirectory::lookup(self, db_session_id.clone()).await
    }

    fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus {
        InMemorySessionDirectory::commit_status(self, key)
    }

    async fn commit(&self, commit: ReplacementCommit) -> Result<ReplacementOutcome, PortError> {
        SessionDirectory::commit_replacement(self, commit).await
    }

    async fn issue_attachment_token(
        &self,
        handle: &DirectoryHandle,
    ) -> Result<AttachmentToken, AttachmentRejection> {
        InMemorySessionDirectory::issue_attachment_token(self, handle)
    }

    async fn attach_client(
        &self,
        handle: &DirectoryHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<(), AttachmentRejection> {
        InMemorySessionDirectory::attach_from_client(
            self,
            handle,
            principal_id,
            client_instance_id,
            token,
        )
        .map(|_| ())
    }
}

/// §7.4 替换请求。
pub struct ContextChangeRequest {
    /// 被替换的旧会话。替换 operation key 由它确定性地派生（§7.4-6 幂等）。
    pub handle: SessionHandle,
    /// 乐观并发闸门：不匹配即拒，绝不基于「大概没变」去替换（§4.4 `:392` 授权它
    /// 用于版本与上下文冲突，§7.1 `:526` 要求排队请求重新校验）。
    pub expected_context_revision: u64,
    /// 期望的新命名空间（连接与对象目标沿用旧会话）。
    pub desired: ExecutionTarget,
    /// 候选 `dbSessionId`，由调用方（directory 的发号方）给出。
    pub candidate_db_session_id: DbSessionId,
    /// 发起替换的归属身份。提交后用它在新会话上挂载，证明新会话真的可路由。
    pub principal_id: PrincipalId,
    pub client_instance_id: ClientInstanceId,
}

/// §7.4 回执：`ContextChangeReceipt { session, replacedSessionId, attachmentToken }`。
///
/// `Debug` 不是装饰：这是一份 `Result` 的成功侧，调用方要能把它 `expect_err` 出去。
#[derive(Debug)]
pub struct ContextChangeReceipt {
    /// 新会话。
    pub session: DirectoryHandle,
    /// 被替换掉的旧会话 id。
    pub replaced_session_id: DbSessionId,
    /// 在新会话上有效的挂载令牌。
    pub attachment_token: AttachmentToken,
}

/// 替换编排器。持有 registry 与 directory 两个依赖：registry 管可见表，
/// directory 持有替换状态机；两者都不反向依赖本模块。
pub struct ContextReplacer {
    registry: Arc<SessionRegistry>,
    directory: Arc<dyn ReplacementDirectory>,
    /// §13.1 `:800` 要求 owner 内存留存的回执产物：**operation key → 已签发的令牌**。
    ///
    /// 为什么这张表必须存在，而不能每次重签：`:799` 限制的是 **durable 记录**（只存请求
    /// 摘要与 receipt 的 durable 投影，不存 `SessionHandle` 原文）；`:800` 紧接着把
    /// SessionView / attachmentToken / receipt 安置在 **owner 内存**里，并要求「响应丢失时，
    /// 在原 runtime 内同键返回同 session/token/候选提交结果」。目录只留摘要，所以令牌原文
    /// 不可能从目录侧取回；能取回它的地方就是 owner，也就是这里。
    ///
    /// 生命周期与 `:800` 一致：随 `ContextReplacer` 生灭，不落盘、不跨进程。
    issued_tokens: Mutex<HashMap<String, AttachmentToken>>,
}

impl ContextReplacer {
    pub fn new(registry: Arc<SessionRegistry>, directory: Arc<dyn ReplacementDirectory>) -> Self {
        Self {
            registry,
            directory,
            issued_tokens: Mutex::new(HashMap::new()),
        }
    }

    /// 取回这张 owner 内存表。中毒不传播：表里存的是纯数据，中毒只能来自**持锁时的
    /// panic**，而本模块持锁期间不做任何可失败或可 panic 的事，因此取回内部值继续用，
    /// 不用 `unwrap`/`expect` 把一个死锁风险换成一次 panic。
    fn issued_tokens(&self) -> MutexGuard<'_, HashMap<String, AttachmentToken>> {
        self.issued_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// §7.4-6 全流程。
    pub async fn replace(
        &self,
        request: ContextChangeRequest,
    ) -> Result<ContextChangeReceipt, RuntimeError> {
        let old_id = request.handle.db_session_id.clone();
        let old_directory_handle = directory_handle(&request.handle);

        // 幂等短路排在**最前**，这是它必须待的位置：替换一旦提交，旧会话就注销了，
        // 任何先定位旧会话的检查（上下文修订、归属）都会先撞上 `UnknownSession`，
        // 重放将永远走不到这里。operation key 只由**旧句柄**决定，目录已经记着答案。
        match self
            .directory
            .commit_status(&ReplacementOperationKey::for_handle(&old_directory_handle))
        {
            CommitStatus::Committed { handle } => {
                // 目录说提交了，注册表这边却**对不上** ⇒ 已提交但无宿主入口的孤儿态。
                // 此时**不能**把目录的句柄当回执发出去。
                //
                // 判据必须比 id 更严，只问「这个 id 在不在册」会被撞号本身骗过去：
                // 候选 id 撞上的是**别人**那一行，那一行确实在册，`is_registered` 为真，
                // 于是重试会把**别人的会话**连同新签的令牌一起发出去——比不发还糟。
                // 真正要问的是「在册的这一行，是不是目录提交的那一代」：epoch 对得上，
                // 才说明它是本轮发布出去的候选。
                //
                // 宁可当场报错——目录已提交这件事记在那里，重试会再次落到这里并得到同样的
                // 答案，不会退化成「按大概重开一次」，也不会退化成「换个人的会话发回去」。
                //
                // ## 为什么是 `InvariantBroken("committedWithoutHostEntry")`，不是别的错
                //
                // 这一格有两个可能的替代品，都比报错糟，所以「报错」本身不是待议项，
                // 待议的只是**用哪个错**：
                //
                // * 发回执：回执里的 `session` 在登记表里不存在，调用方拿到 `Ok`，紧接着
                //   `session_view` 撞 `UnknownSession`——看起来成功、实际用不了。§13.1 `:800`
                //   的「同键返回同 session」承诺的前提是那一代会话**真的在**，前提没了还照发，
                //   等于用一个可验证的失败换一个不可验证的失败。
                // * 退化成「按大概重开一次」：把幂等键变成随机数，而且每重试一次白吃一格
                //   额度，泄漏到顶之后这个连接再也开不出会话。
                //
                // 至于为什么落在 `InvariantBroken` 而不是 `UnknownSession`/`SessionLost`：
                // `UnknownSession` 说的是「你给的句柄查不到」，会把责任指回调用方，而这里
                // 调用方给的东西**是对的**；`SessionLost` 说的是「会话曾经存在、后来没了」，
                // 而撞号那一格里那一行从头到尾活着。两者都会把排查引向错误的方向。
                // `InvariantBroken` + 一个逐字稳定的码，是唯一能同时表达「这是宿主自己的
                // 两份账脱节了」和「调用方可以据此判断再重试也没用」的形状。
                //
                // 代价要写明：这一格之后调用方**永远**拿不到回执，只能自己收敛（重新取号
                // 发起一次全新的替换）。这是有意接受的——目录的 committed 记录不由宿主撤销，
                // 宿主也无法证明「那一代会话本该还活着」，替它重开就是伪造。
                //
                // 覆盖：`tests/registry_rejection.rs` 的「候选发布失败时额度回到原值且撞号的
                // 那一行不动」（在册但是别人的世代）与「目录记着已提交但注册表已无该行时同键
                // 重放不得签发回执也不得凭空重开」（连在册都不在）。后者特意断言**额度不动**、
                // **表项不变**：报错若带着副作用，「孤儿态」就会变成「额度泄漏源」。
                match self.registry.epoch_of(&handle.db_session_id) {
                    Ok(epoch) if epoch_string(epoch.get()) == handle.runtime_epoch.as_str() => {}
                    other => {
                        warn!(
                            candidate = %handle.db_session_id,
                            committed_epoch = %handle.runtime_epoch,
                            registered_epoch = ?other.as_ref().ok().map(|epoch| epoch_string(epoch.get())),
                            "§7.4-6 幂等短路被拒：目录已提交但注册表没有这一代（发布失败的孤儿态\
                             或 id 已被他人占用），不签发回执"
                        );
                        return Err(RuntimeError::InvariantBroken("committedWithoutHostEntry"));
                    }
                }
                // §13.1 `:800`：「响应丢失时，在原 runtime 内**同键返回同 session/token/
                // 候选提交结果**」。所以这里是**回放同一枚令牌**，不是重签一枚。
                //
                // 上一版本在这里重签，理由写的是「目录只留摘要」——那句话只说明令牌原文
                // 不在**目录**里，不说明它不在 **owner** 里（`:800` 把 SessionView /
                // attachmentToken / receipt 明确安置在 owner 内存）。按重签处理还有一个
                // 实害：网络层丢响应时调用方手上只有**最初**那枚，而重签会换掉目录里的摘要
                // 让它立刻作废，于是「替换已提交」变成「调用方永远附着不上」，而重试又是
                // 唯一的出路——幂等键在这种情况下等于没有。
                let token = self
                    .replayed_token(
                        &ReplacementOperationKey::for_handle(&old_directory_handle),
                        &handle,
                    )
                    .await?;
                return Ok(ContextChangeReceipt {
                    session: handle,
                    replaced_session_id: old_id,
                    attachment_token: token,
                });
            }
            // 上一次回滚了：这**不是**重试，是一个新请求，重新走一遍。
            CommitStatus::RolledBack { .. } | CommitStatus::Pending => {}
        }

        // 定位旧会话。定位不到就是 `UnknownSession`——**不**退化成「找相似 id 的那个」。
        let old_view = self.registry.session_view(&request.handle).await?;
        if old_view.context_revision.get() != request.expected_context_revision {
            return Err(RuntimeError::ContextRevisionMismatch {
                expected: request.expected_context_revision,
                actual: old_view.context_revision.get(),
            });
        }
        let old_owner = self
            .directory
            .owner_of(&old_id)
            .await
            .map_err(port_error)?
            .ok_or_else(|| RuntimeError::UnknownSession(old_id.to_string()))?;

        if self.registry.remaining_quota() == 0 {
            return Err(RuntimeError::BudgetExhausted("replacementNeedsSlot"));
        }

        // ① 候选物理资源：开了、起了 actor，但**不在可见表里**。
        let (actor, view, runtime_epoch) = self
            .registry
            .open_candidate(open_request(&request, &old_view, &old_owner))
            .await?;
        let candidate_handle = view.handle.clone();
        let candidate_directory_handle = directory_handle(&candidate_handle);
        let new_owner = SessionOwner {
            db_session_id: request.candidate_db_session_id.clone(),
            runtime_epoch: PlatformEpoch::new(epoch_string(runtime_epoch.get())),
            resource_epoch: old_owner.resource_epoch + 1,
            ..old_owner.clone()
        };

        // ②③④ 旧 actor 举闸门。此后旧会话不再发新执行，句柄与资源仍原封不动。
        if let Err(error) = self.registry.hold_for_replacement(&request.handle).await {
            self.abort_candidate(&actor, &candidate_handle).await;
            return Err(error);
        }

        // §12 Prepared / Committed。两步之间任何一步失败都走「提交前失败」分支。
        let key = ReplacementOperationKey::for_handle(&old_directory_handle);
        if let Err(error) = self
            .directory
            .commit(ReplacementCommit {
                old: old_directory_handle.clone(),
                new_owner: new_owner.clone(),
                operation: ReplacementOperation::Prepared,
            })
            .await
        {
            self.abort_candidate(&actor, &candidate_handle).await;
            let _ = self
                .registry
                .resume_after_replacement(&request.handle)
                .await;
            return Err(port_error(error));
        }
        if let Err(error) = self
            .directory
            .commit(ReplacementCommit {
                old: old_directory_handle.clone(),
                new_owner: new_owner.clone(),
                operation: ReplacementOperation::Committed,
            })
            .await
        {
            // Prepared 已落 ⇒ 必须回滚，不能留一条悬空的 replacement 记录。
            let _ = self
                .directory
                .commit(ReplacementCommit {
                    old: old_directory_handle.clone(),
                    new_owner: new_owner.clone(),
                    operation: ReplacementOperation::RolledBack,
                })
                .await;
            warn!(%key, "§7.4-6 提交失败，已回滚到候选前状态");
            self.abort_candidate(&actor, &candidate_handle).await;
            let _ = self
                .registry
                .resume_after_replacement(&request.handle)
                .await;
            return Err(port_error(error));
        }

        // ⑥ §9.4：旧会话走**唯一**那条释放例程。已提交，所以这里失败也不回头——
        // 失败的语义是「旧会话已丢失/已隔离」，表项在 `close_registered` 里一并注销，
        // 新会话仍是唯一真相。
        if let Err(error) = self
            .registry
            .close_registered(&request.handle, CloseMode::RollbackAndClose)
            .await
        {
            warn!(
                old = %old_id,
                %error,
                "§7.4-6 提交后释放旧会话未得 Clean：新会话已是既成事实，不倒退"
            );
        }

        // 候选此刻才进可见表。
        //
        // 失败（候选 id 与在册行撞号）时**必须**显式退还额度：候选从未进过表，
        // `forget` 对它无效——它不是「从表里摘掉」，所以额度只有 `refund_candidate`
        // 这一条退路。漏掉这一步，这一格额度就永久卡死，且没有任何后续路径会想起它。
        if let Err(error) =
            self.registry
                .publish_candidate(new_owner.worker_id.clone(), runtime_epoch, view, actor)
        {
            self.registry.refund_candidate();
            warn!(
                %error,
                "§7.4-6 候选发布失败：额度已退还；新会话不可见，已提交但无宿主入口"
            );
            return Err(error);
        }

        // 给**已发布**的新会话签发它自己的令牌，再拿这枚令牌挂载：回执里的
        // `attachmentToken` 必须**真的**在新会话上作数，否则这个字段就是一句空话。
        // 签发在提交之后、挂载之前：屏障上的候选还路由不到，谁去签都是拒签。
        let token = self
            .directory
            .issue_attachment_token(&candidate_directory_handle)
            .await
            .map_err(attachment_error)?;
        self.directory
            .attach_client(
                &candidate_directory_handle,
                request.principal_id.clone(),
                request.client_instance_id.clone(),
                token.clone(),
            )
            .await
            .map_err(attachment_error)?;

        // §13.1 `:800` 的 owner 内存留存就在这一笔：**签发成功后**才登记，登记的是
        // **即将发进回执的那一枚**。写在这里而不是 `attach_client` 之前，是因为挂载失败
        // 会把这次替换整体退成错误——那种情况下不该有一枚「回执里的」令牌留在表里。
        // key 取旧句柄派生的那一个：幂等判的是「同一个被替换的旧会话」，不是新会话。
        self.issued_tokens().insert(
            ReplacementOperationKey::for_handle(&old_directory_handle)
                .as_str()
                .to_owned(),
            token.clone(),
        );

        Ok(ContextChangeReceipt {
            session: candidate_directory_handle,
            replaced_session_id: old_id,
            attachment_token: token,
        })
    }

    /// 幂等短路上的令牌来源：优先回放 owner 内存里那一枚实在的令牌（§13.1 `:800`），
    /// 没有才签发并登记。
    ///
    /// 「有缓存」不等于「可以发」。`:800` 说的留存期限是「令牌过期或 runtime 终止」，
    /// 而令牌的死活由目录侧那条摘要是权威；缓存只证明「我们曾经发过这一枚」，不证明
    /// 「它现在还能用」。所以命中缓存后先确认目录**还认不认**这一代会话：
    ///
    /// - 认得 ⇒ 直接发回同一枚。目录里的摘要没有被动过（动过就会换摘要、换掉之后旧
    ///   令牌当场作废），所以调用方手上那枚仍然作数。
    /// - 不认 ⇒ 这枚令牌已经死了。把死令牌当成功发出去，等于把失败推迟到附着处；于是
    ///   丢掉缓存条目，退回签发，让 `issue_attachment_token` 给出**它自己的**拒绝
    ///   （`NotRoutable`）——和首次走到这里时的错误完全同型。
    ///
    /// 问的是 `owner_of`（查得到 owner 才说在），不是「表里有没有这一条」：后者对一条
    /// 被关闭的条目同样为真，按它判等于没验。
    ///
    /// ★ 能走到「不认」这一支的前提是条目**已经不在目录里**（作废会删条目）。只被关闭
    /// （§12 的 `release`）的候选够不着这里：调用方在取本函数之前先问过一次提交结果，
    /// `commit_status` 见已提交就把候选重新 `publish()` 回可路由，于是关闭的候选在重放
    /// 途中自己活了回来。这条口径差异属目录侧、只报告不修，记在分支台账的待裁定项里。
    async fn replayed_token(
        &self,
        key: &ReplacementOperationKey,
        committed: &DirectoryHandle,
    ) -> Result<AttachmentToken, RuntimeError> {
        let cache_key = key.as_str().to_owned();
        let remembered = self.issued_tokens().get(&cache_key).cloned();
        if let Some(token) = remembered {
            if self
                .directory
                .owner_of(&committed.db_session_id)
                .await
                .map_err(port_error)?
                .is_some()
            {
                return Ok(token);
            }
            warn!(
                candidate = %committed.db_session_id,
                "§13.1 幂等重放：owner 内存里留存的令牌所属会话已从目录中消失，\
                 丢弃该条目并按拒绝处理，不把死令牌当成功回执发回"
            );
            self.issued_tokens().remove(&cache_key);
        }
        let token = self
            .directory
            .issue_attachment_token(committed)
            .await
            .map_err(attachment_error)?;
        self.issued_tokens().insert(cache_key, token.clone());
        Ok(token)
    }

    /// §7.4-6 第 10 项：提交前失败 ⇒ 销毁候选。
    ///
    /// 销毁**同样**走 §9.4：候选若带句柄消失，也必须先在它自己登记的那条资源上
    /// 终结，而不是直接丢一个物理资源。
    async fn abort_candidate(&self, actor: &SessionActor, candidate: &SessionHandle) {
        if let Err(error) = actor.destroy_candidate(candidate).await {
            warn!(
                candidate = %candidate.db_session_id,
                %error,
                "§7.4-6 候选销毁未得 Clean：额度照退，物理资源由 actor 退出收尾"
            );
        }
        self.registry.refund_candidate();
    }
}

/// 候选的 `OpenRequest`：沿用旧会话的连接/归属/配置，只换命名空间。
///
/// `owner` 取自 `SessionView`（runtime 侧的权威表示），不是目录投影里的
/// `OwnerRef`——那是另一套更窄的枚举，抄过来会丢字段。
fn open_request(
    request: &ContextChangeRequest,
    old_view: &SessionView,
    old_owner: &SessionOwner,
) -> OpenRequest {
    OpenRequest {
        db_session_id: request.candidate_db_session_id.clone(),
        worker_id: old_owner.worker_id.clone(),
        connection_id: old_owner.connection_id.clone(),
        config_revision: old_view.config_revision.clone(),
        owner: old_view.owner.clone(),
        initial_target: request.desired.clone(),
        expires_at: old_view.expires_at.clone(),
        idle_deadline_ms: None,
    }
}

/// runtime 句柄 → 目录句柄。**确定性**换算：同一个 runtime 句柄永远得到同一个
/// 目录句柄，因而 `ReplacementOperationKey::for_handle` 也永远得到同一个 key。
fn directory_handle(handle: &SessionHandle) -> DirectoryHandle {
    DirectoryHandle::new(
        handle.db_session_id.clone(),
        PlatformEpoch::new(epoch_string(handle.runtime_epoch.get())),
    )
}

fn port_error(error: PortError) -> RuntimeError {
    match error {
        PortError::NotFound(id) => RuntimeError::UnknownSession(id),
        PortError::TokenInvalid => RuntimeError::CloseRejected("attachmentTokenRejected"),
        // `RuntimeError` 只收 `&'static str`，所以端口原话进日志，错误面上留稳定理由。
        other => {
            warn!(reason = %other, "§7.4-6 目录端口拒绝了替换操作");
            RuntimeError::InvariantBroken("directoryRejectedReplacement")
        }
    }
}

fn attachment_error(rejection: AttachmentRejection) -> RuntimeError {
    warn!(?rejection, "§7.4-6 无法给新会话签发 attachment 令牌");
    RuntimeError::CloseRejected(attachment_reason(&rejection))
}

fn attachment_reason(rejection: &AttachmentRejection) -> &'static str {
    match rejection {
        AttachmentRejection::NoToken => "attachmentTokenMissing",
        AttachmentRejection::NoTokenIssued => "attachmentTokenNotIssued",
        AttachmentRejection::TokenMismatch => "attachmentTokenRejected",
        AttachmentRejection::PrincipalMismatch => "attachmentPrincipalMismatch",
        AttachmentRejection::OwnerMismatch => "attachmentOwnerMismatch",
        _ => "attachmentRejected",
    }
}
