# p3-gateway-owner-binding 进度台账（交付即销毁）

目标：闭合 P3 出口闸门唯一缺失项 **CM-06 前端伪造 owner（H/W）**，并连带覆盖
CM-04 / CM-05 的 H 层。验收标准出处：
`docs/architecture/platform/connection-management.md:885-901`（本轨**不修改**该文档）。

- 分支：`feature/p3-gateway-owner-binding`
- 基线：`f1d843271510eb7b64f5a3a435e1cc4ea6603206`
- 工作树：`.worktrees/datazen-p3-gateway-owner-binding`
  （派单里写的 `datazen-gateway-owner-binding` 不存在，见「派单事实勘误」）

## 三态

### 已完成

| 项 | 提交 | 说明 |
| --- | --- | --- |
| `DenialVisibility` + `AuthorizationDenial.visibility` | `4549befbe` | 拒绝分「透明/隐藏」两类；隐藏类投影为 `UnknownSession` |
| `owner_binding::OwnerMatchAuthorizer` + `resolve_owned_session` | `4549befbe` | 真正的生产实现，非 `Always*` 族；三处闸门共用一个 resolver |
| `gateway/mod.rs` 接入（accept / dispatch / cancel） | `4549befbe` | 798 行，未突破 800 行红线 |
| CM-04/05/06 的 H 层集成门禁（14 项） | `e5f7a352d` | `packages/runtime/tests/owner_binding.rs` |
| 夹具拆子模块，回到 800 行以内 | `ee0ccb74c` | 567 + 343 |
| 台账 | 本提交 | |

### 进行中

- `request.rs` 的 CM-06 编译期/反序列化期负例（`#[cfg(doctest)]` 三个 `compile_fail`）。
- 三条门禁的全量复跑、变异证明（always-allow 反转 + `!=` 反转，各带负对照）。

### 未开始

- W 层（CM-06 的 W 半边）、P7 侧的 `ArtifactStore` 订阅/下载（CM-61/CM-64）。

## 门禁（自验，逐字结论行）

| 门禁 | 结论行 |
| --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 待复跑 |
| `cargo test -p datazen-runtime`（全量） | 待复跑 |
| `pnpm typecheck` | 待复跑 |
| `cargo test -p datazen-runtime --test owner_binding`（本轨新增） | `test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |

判绿按 `passed` 计数，不按退出码；并确认 `running N` 且 N>0。

## 本轨未修、归属已登记

### 1. `OwnerRef::Job` 不含主体 ⇒ CM-06「U1 指向 U2 的 job」在 H 层不可闭合

- 事实：`packages/runtime/src/connection/types.rs:390-394`，`OwnerRef::Job` 的字段
  只有 `organization_id` / `job_id` / `stage_id`，**没有 `principal_id`**。同组织内
  U1 与 U2 在该变体上完全同形，`Authorizer::authorize` 的入参里也没有
  「本次请求被授权操作哪个 job」。
- 行为已钉住：`tests/owner_binding.rs` 的
  `cm06_a_job_owner_carries_no_principal_so_same_organization_peers_pass_the_gate`
  （同组织 peer **确实被受理**）+ `cm06_the_job_owner_variant_field_list_is_pinned`
  （字段表结构守卫，字段一变就红）。跨组织那一半已闭合：
  `cm06_a_job_owned_by_another_organization_is_refused`。
- 真正闭合它的实现在上一层：`packages/application/src/identity_policy.rs:147-153`
  的 `check_owner(ctx, owner, authorized_job)` 用调用方显式声明的 `authorized_job` 与
  `owner.job_id` 比对。**归属：application 层（§5.1(3)）。本轨不改该文件。**

### 2. 取消路径残留存在性预言机

- 事实：`gateway/mod.rs:641-644` 的执行记录查表发生在 `:655` 的 `verify_binding`
  与 `:657` 的 `owned_view` **之前**。故「`executionId` 根本不存在」得
  `CancelFailed("unknownExecution")`（`api_code()` 为 `None`），「存在但不归我」得
  `sessionNotFound` —— 两个对外码可区分，攻击者据此判定该 `executionId` 是否存在。
- 为何本轨不修：修它要改 `CancelFailed` 的对外语义，而
  `tests/registry_audit.rs:103`、`tests/registry_cancel.rs:140,289,311`、
  `tests/p3_session_port_contract.rs:428,472`、`tests/gateway_contract/cancel.rs:74,118`
  逐条钉死了当前语义；且与 `datazen-p3-cm60-pressure-drain` @ `eb607ad88`、
  `datazen-p3-cm70-fu2-token-leak` @ `f1d843271`、`ta-cm60` @ `eb607ad88` 共享。
- 本轨做到哪一步：拒绝时**驱动 `cancel` 调用次数 = 0** 已由
  `cm05_cancelling_a_foreign_execution_never_reaches_the_driver` 钉死。
- **归属：cancel 语义所属轨（建议 P7 或独立的 cancel 闸门轨）。**

### 3. CM-04 的 `executeAtTarget` 半边

- 事实：`executeAtTarget` 是 **application 层契约，仓库内零实现**
  （`packages/application/src/sessions.rs:104-107` 声明，
  `impl ConnectionUseCases` 全仓不出现）。runtime 侧的
  `ExecuteAtTargetRequest`（`connection/session.rs:184-190`）是
  `{target, call, idempotency_key}`，其 application DTO 另需
  `expected_config_revision: Counter`（CAS，无 serde 默认值）。
- 故「向 `executeAtTarget` 传未知 profile ⇒ 不可见配置错误」在 H 层**没有可注入的
  调用点**，不编造。CM-04 的 `executeInSession` 半边已闭合并有门禁。
- **归属：application / 组装层。**

### 4. `registry/actor.rs` 的第二处未文档化 `SessionClosed`

- 事实：`registry/actor.rs:176-189` 的 `fn exec<T>`，channel 发送失败返回
  `UnknownSession`（有文档），而 `rx.await.unwrap_or(Err(SessionClosed(..)))`
  是**第二处、未在 `registry/port.rs:42-45` 失败集合里声明的** `SessionClosed`。
- 本轨处理：`resolve_owned_session` 已把它一并归一到 `UnknownSession`（两者载荷都
  来自调用方给的 id，哨兵逐字节相同）。是否补进 trait 的失败集合，属于 registry
  契约本身。**归属：registry 契约轨。**

## 派单事实勘误（逐条）

1. **工作树路径**：派单写 `.../datazen-gateway-owner-binding`，实际是
   `.../datazen-p3-gateway-owner-binding`。前者不存在。
2. **`gateway/mod.rs` 行数**：派单说 799，实测 **798**（`wc -l`，改前 807，压到 798）。
3. **`OwnerRef` 定义位置**：派单只给了用法 `connection/session.rs:151-163`。
   定义处在 **`packages/runtime/src/connection/types.rs:382`**（`pub enum OwnerRef`），
   doc 注释起于 `:376`；经 `src/connection/mod.rs:35` 的 `pub use types::{...}`
   再导出；作为字段使用在 `src/connection/session.rs:155` 的 `pub owner: OwnerRef`。
   四个变体字段见 `types.rs:383-401`。
4. **CM-01「编译期负例」是先例**：核实为**散文级**——仓库里没有 compile-fail
   fixture，只有 `connection-management.md:867-871` 的文字。本轨按 CM-01 的**意图**
   实现（`compile_fail` doctest + 反序列化负例 + 源码结构守卫），不援引一个不存在
   的先例。
5. **「本轮零产出」不成立**：核查当时 `git status --porcelain` 为
   ` M packages/runtime/src/gateway/mod.rs`、` M packages/runtime/src/gateway/provenance.rs`、
   `?? packages/runtime/src/gateway/owner_binding.rs`（19282 字节）；单测结论行
   `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 436 filtered out`。

## 已知环境问题（非本轨）

- 仓库根 `cargo fmt` 必失败：gitignored 的 `src-tauri/src/driver_init.rs` 在每棵
  工作树里都缺失。故门禁一律用 `cargo fmt -p datazen-runtime -- --check`。