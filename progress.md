# p3-gateway-owner-binding 进度台账（交付即销毁）

目标：闭合 P3 出口闸门唯一缺失项 **CM-06 前端伪造 owner（H/W）**，并连带覆盖
CM-04 / CM-05 的 H 层。验收标准出处：
`docs/architecture/platform/connection-management.md:885-901`（本轨**不修改**该文档）。

- 分支：`feature/p3-gateway-owner-binding`
- 基线：`f1d843271510eb7b64f5a3a435e1cc4ea6603206`
- 交付 HEAD：`790c323af7a30fbc75df4a09ff55122514a4b30d`
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
| CM-06 编译期/反序列化期负例（4 个 `compile_fail`） | `34669fb1f` | `request_cm06_negatives.rs` 236 行；`request.rs` 599 行 |
| CM-70 反面证据扫描登记新增文件 | `a221fe45f` | `tests/cm70_no_disk.rs` |
| 契约不变量扫描登记新增文件 | `790c323a` | `tests/gateway_contract/invariants.rs` |
| 三条门禁全量复跑 + 变异证明 + 负对照 | 本提交 | 结论行见下表 |

### 进行中

无。

### 未开始

- W 层（CM-06 的 W 半边）、P7 侧的 `ArtifactStore` 订阅/下载（CM-61/CM-64）。

## 门禁（自验，逐字结论行）

复跑时 HEAD 前后一致、`git status --porcelain` 为空，两次均记录。

| 门禁 | 结论行 |
| --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 无 diff，`EXIT=0` |
| `cargo test -p datazen-runtime --no-fail-fast`（全量） | `EXIT=0`；23 个目标合计 **710 passed / 0 failed**，其中 `unittests src/lib.rs` = `ok. 441 passed`、**`tests/owner_binding.rs` = `ok. 14 passed`**、**`Doc-tests datazen_runtime` = `ok. 7 passed`** |
| `pnpm typecheck` | `EXIT=0`，`tsc --noEmit` 无输出；末行 `$ tsc -p tsconfig.pack-ep.json --noEmit` |
| `cargo test -p datazen-runtime --test cm70_no_disk` | `running 13 tests` / `test result: ok. 13 passed; … 0 filtered out` |
| `cargo test -p datazen-runtime --test gateway_contract` | `running 51 tests` / `test result: ok. 51 passed; … 0 filtered out` |
| `cargo test -p datazen-runtime --test owner_binding`（本轨新增） | `running 14 tests` / `test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |

判绿按 `passed` 计数，不按退出码；并确认 `running N` 且 N>0。

注：`pnpm typecheck` 直接跑会因工作树 `node_modules` 是指向主仓的符号链接而报
`workspace hoist directory is not a real directory`；用
`pnpm --config.verify-deps-before-run=false typecheck` 跳过 pnpm 的依赖预校验即可，
门禁本体（`tsc --noEmit`）照跑。

## 变异证明（每条都带负对照；全部已还原，工作树干净）

基线：`cargo test -p datazen-runtime --test owner_binding` = `ok. 14 passed`，
`cargo test -p datazen-runtime --doc` = `ok. 7 passed`。

### Authorizer 比较（`owner_binding.rs`）

| 变异 | 结果 |
| --- | --- |
| 变异1 两处 `if owner != … {` → `if false {`（always-allow） | `EXIT=101`，`FAILED. 5 passed; 9 failed` |
| 变异2 两处 `!=` → `==`（双向反转） | `EXIT=101`，`FAILED. 7 passed; 7 failed`；**含 `cm06_the_real_owner_is_still_accepted` 失败** ⇒ 该检查双向承重 |
| 变异2b 只反转主体比较 | `EXIT=101`，`FAILED. 5 passed; 9 failed` |
| 变异3 只反转组织比较（**负对照**） | `EXIT=101`，`FAILED. 7 passed; 7 failed`，**失败集合与变异2 不同** ⇒ 组织比较与主体比较各自独立被覆盖 |

### 编译期负例（`request_cm06_negatives.rs` / `request.rs`）

| 变异 | 结果 |
| --- | --- |
| 删掉负例里的 `owner: "forged",` 一行 | 仍 `7 passed`、`EXIT=0` ⇒ 负例转红**只**因为该字段不存在（对照体保持诚实） |
| 变异B(v5) 给 `ExecutionRequest` 加 `pub owner: String`、构造函数补 `owner: String::new()`、对照体字面量同步、负例字面量改 `owner: "forged".to_owned()` | `EXIT=101`，`FAILED. 6 passed; 1 failed`，失败于 line 29：`Test compiled successfully, but it's marked \`compile_fail\`.` |
| 变异A 把 `mod payload::Body` 三个字段全改 `pub` | `EXIT=101`，`FAILED. 6 passed; 1 failed`，失败于 line 116，同上文案 |
| 变异D(v2) 给 `ExecutionRequest` 加显式 `Deserialize<'de>` 实现 | `EXIT=101`，`FAILED. 6 passed; 1 failed`，失败于 line 196，同上文案 |

被作废的「转红但理由不对」三次（均已丢弃重做）：只加字段不改构造函数 ⇒
`error[E0063]: missing field \`owner\` in initializer of \`ExecutionRequest\``；
改了构造函数没改 doctest 对照体字面量 ⇒ 同样 E0063；改了对照体但负例仍写
`owner: "forged"`（`&str` 对 `String` 字段）⇒ **仍绿 7 passed**，这本身就是
「错误码后缀不被强制」的一次活证；`Deserialize<'_>` 省略生命周期写法 ⇒
`error[E0726]: implicit elided lifetime not allowed here`（lib 编译失败，不算 doctest 证据）。

### 工具链事实（本轮实测）

**rustdoc 1.90.0 不强制 `compile_fail,EXXXX` 的错误码后缀**。把 `compile_fail,E0560`
改成 `compile_fail,E0308`，两次都仍 `7 passed`、`EXIT=0`。因此每个负例都配了一个
**能编译的对照体**：若真值变绿，对照体随即转红，行数不变。

### 两个被纠正的事实

1. Rust 私有性是**模块级**而非函数级：把 `struct Body` 与读它的 `fn main` 放同一模块
   时私有字段访问**能编译**（先前一版负例因此是绿的）。必须把 `Body` 放进嵌套
   `mod payload { … }`，访问者留在 crate 根，才真正触发。
2. 私有字段从外部访问报的是 **E0616**（`field \`x\` of struct \`Body\` is private`），
   不是 E0609（私有方法）。首版负例按 E0609 写，方向就错了。

## 被门禁挡下的两处完整性登记（本轨新增文件必须登记，否则证据静默失效）

`every_gateway_source_file_is_scanned`（`tests/cm70_no_disk.rs`）与
`gateway_sources_obey_the_locked_invariants`（`tests/gateway_contract/invariants.rs`）
都断言 `src/gateway/` 的实际文件集与常量完全一致。新增两个文件后二者直接红。
这是对两处**冻结文件**的唯一改动，方向是收紧而非放宽，且都带负对照：

| 登记 | 负对照（逐字） |
| --- | --- |
| `cm70_no_disk.rs` 的 `SOURCES` 加两条 `include_str!` | 往 `owner_binding.rs` 栽 `std::fs::File::open` ⇒ `EXIT=101`，`src/gateway/owner_binding.rs 里出现了std 文件系统（\`std::fs\`）：令牌与回执只在内存里，不得落盘` |
| `invariants.rs` 的 `expected` 按文件性质分流：`owner_binding.rs` 进**生产文件**组（它 `pub mod` 进生产二进制，本就该受生产路径禁用词扫描），`request_cm06_negatives.rs` 进**测试专用**组并同列 `test_only`（它只编进 `#[cfg(doctest)]`，与该列表注释「测试专用」同类） | 往 `owner_binding.rs` 生产段栽 `panic!` ⇒ `EXIT=101`，`owner_binding.rs:235 生产路径出现 panic!(：fn _planted() { panic!("x"); }` |

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

### 5. （本轮新发现）两处文件集登记绊线，无 CI 提示即静默失效

上面「被门禁挡下的两处登记」本是**既有设计**：漏登记会让「令牌不落盘」与
「生产路径禁用词」两条证据对新文件**悄悄失效**。但它们只在**本地全量跑**时
触发——单跑 `--test owner_binding` 或 `--doc` 不会碰到。**归属：CI 接线
（P3 出口闸门跑全量时补 `cm70_no_disk` / `gateway_contract` 两条目标）。**

## 派单事实勘误（逐条）

1. **工作树路径**：派单写 `.../datazen-gateway-owner-binding`，实际是
   `.../datazen-p3-gateway-owner-binding`。前者不存在。
2. **`gateway/mod.rs` 行数**：派单说 799，实测 **798**（`wc -l`，改前 807，压到 798）。
3. **`OwnerRef` 定义位置**：派单只给了用法 `connection/session.rs:151-163`。
   定义处在 **`packages/runtime/src/connection/types.rs:382`**（`pub enum OwnerRef`），
   doc 注释起于 `:376`；经 `src/connection/mod.rs:35` 的 `pub use types::{...}`
   再导出；作为字段使用在 `src/connection/session.rs:155` 的 `pub owner: OwnerRef`。
   四个变体字段见 `types.rs:383-401`。**另有一个同名不同类型的 `OwnerRef`** 在
   `packages/platform-api/src/context.rs:180`，与本轨无关，勿混。
4. **CM-01「编译期负例」是先例**：核实为**散文级**——仓库里没有 compile-fail
   fixture，只有 `connection-management.md:867-871` 的文字。本轨按 CM-01 的**意图**
   实现（`compile_fail` doctest + 反序列化负例 + 源码结构守卫），不援引一个不存在
   的先例。
5. **「本轮零产出」不成立**：核查当时 `git status --porcelain` 为
   ` M packages/runtime/src/gateway/mod.rs`、` M packages/runtime/src/gateway/provenance.rs`、
   `?? packages/runtime/src/gateway/owner_binding.rs`（19282 字节）；单测结论行
   `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 436 filtered out`。
6. **`ApiErrorCode` 未从 `datazen_runtime::connection` 再导出**：`connection/mod.rs:22`
   只有 `pub use error::{ApiError, ProviderError, RuntimeError};`。集成测试须写
   `use datazen_platform_api::error::ApiErrorCode;`。
7. **网关无生产组装根**：`ExecutionGateway::(new|with_submission_tokens)` 全仓 10 处命中
   **全在测试/夹具**，`src-tauri` 里 **0 处**。故本轨的 `Authorizer` 尚无生产注入点，
   下游接线时必须显式传 `OwnerMatchAuthorizer::shared()`。

## 已知环境问题（非本轨）

- 仓库根 `cargo fmt` 必失败：gitignored 的 `src-tauri/src/driver_init.rs` 在每棵
  工作树里都缺失。故门禁一律用 `cargo fmt -p datazen-runtime -- --check`。
- 工作树 `node_modules` 是指向主仓的符号链接，pnpm 的依赖预校验会拒绝
  `workspace hoist directory is not a real directory`；用
  `pnpm --config.verify-deps-before-run=false typecheck` 绕过。

---

合并进 main 前必须删除本文件。