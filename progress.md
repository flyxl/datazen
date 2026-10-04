# p3-gateway-owner-binding 进度台账（交付即销毁）

目标：闭合 P3 出口闸门唯一缺失项 **CM-06 前端伪造 owner（H/W）**，并连带覆盖
CM-04 / CM-05 的 H 层。验收标准出处：
`docs/architecture/platform/connection-management.md:885-901`（本轨**不修改**该文档）。

- 分支：`feature/p3-gateway-owner-binding`
- 基线：`f1d843271510eb7b64f5a3a435e1cc4ea6603206`
- 交付 HEAD（代码，已过全部门禁）：`bf7c35b7c4d7fa53c5957dccd6a2833865e85833`
  （本文件自身的提交在其后，故本行的 sha 恒比分支 HEAD 少一个）
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

★ 每轮复跑**首尾各记一次** HEAD 与工作区（`git status --porcelain`），
证明运行期间没有别人动过树。本轮（修复轮）逐字记录：

```
HEAD_START = bf7c35b7c4d7fa53c5957dccd6a2833865e85833
PORCELAIN_START = [ M progress.md]        ← 唯一的改动就是这份台账本身
merge-base main HEAD = f1d843271510eb7b64f5a3a435e1cc4ea6603206
HEAD_END   = bf7c35b7c4d7fa53c5957dccd6a2833865e85833   ← 与 HEAD_START 相同
PORCELAIN_END = [ M progress.md]
```

| 门禁 | 结论行（本轮实测，每条**只跑一次**，未重跑到绿） |
| --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | `EXIT=0`，日志 0 字节（无 diff） |
| `cargo test -p datazen-runtime --no-fail-fast`（全量） | `EXIT=0`；**24** 个目标合计 **715 passed / 0 failed**（基线 710，+5 = `production_wiring` 的 5 条门禁） |
| `cargo test -p datazen-runtime --lib` | `EXIT=0`，`test result: ok. 441 passed; 0 failed; … 0 filtered out` |
| `cargo test -p datazen-runtime --test owner_binding`（本轨） | `running 19 tests` / `EXIT=0` / `test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `cargo test -p datazen-runtime --test cm70_no_disk` | `running 13 tests` / `EXIT=0` / `test result: ok. 13 passed; …` |
| `cargo test -p datazen-runtime --test gateway_contract` | `running 51 tests` / `EXIT=0` / `test result: ok. 51 passed; …` |
| `cargo test -p datazen-runtime --doc` | `running 7 tests` / `EXIT=0` / `test result: ok. 7 passed; …` |
| `pnpm --config.verify-deps-before-run=false typecheck` | `EXIT=0`，`error TS` 命中 **0**，末行 `$ tsc -p tsconfig.pack-ep.json --noEmit` |

判绿按 `passed` 计数，不按退出码；并确认 `running N` 且 N>0（本轮每一条的 `running N` 都> 0，
不是 `0 passed` 的空转绿）。仓库有约 12.5% 的偶发失败率，**本轮没有为了变绿而重跑过任何一条**；
若验收方复跑遇到零星失败，那是仓库 flake，不是本轨回归——重跑一次确认即可。

本轮门禁与上一轮唯一的差异是 `progress.md` 这份台账本身。实测
`git grep -n progress.md -- packages/runtime/tests` **零命中**：`cm70_no_disk` 的
`SOURCES` 登记表与 `gateway_contract/invariants.rs` 的 `production_lines` 名单
**都不含它**，它也不参与编译与 typecheck，所以改动它不可能移动任何门禁。

注：`pnpm typecheck` 直接跑会因工作树 `node_modules` 是指向主仓的符号链接而报
`workspace hoist directory is not a real directory`；用
`pnpm --config.verify-deps-before-run=false typecheck` 跳过 pnpm 的依赖预校验即可，
门禁本体（`tsc --noEmit`）照跑。

## 变异证明（每条都带负对照；全部已还原，工作树干净）

基线（HEAD `bf7c35b7`，2026-08 的修复轮实测）：
`cargo test -p datazen-runtime --test owner_binding` = `test result: ok. 19 passed; 0 failed`，
`cargo test -p datazen-runtime --doc` = `test result: ok. 7 passed; 0 failed`。

> ★ **本节整表已在修复轮重测并重写。** 上一版的三条 Authorizer 变异结论里有两条被实测
> 推翻（下面「被推翻的旧结论」一节逐条列出），**数字与失败集合以本表为准**。

### Authorizer 比较（`owner_binding.rs`）

变异位点实测在 `packages/runtime/src/gateway/owner_binding.rs`：
`:171` `check_organization(...)?;`、`:172` `check_principal(...)`、
`:190` `if owner != principal.organization_id() {`、`:204` `if owner != principal.principal_id() {`。

| 变异 | 结果（`--test owner_binding`） |
| --- | --- |
| A 删掉 `:171-172` 两个调用点（两个 `fn` 体变死代码，`Job` 分支仍走组织比较） | `EXIT=101`，`FAILED. 12 passed; 7 failed` |
| B 只反转组织比较（`:190` `!=` → `==`） | `EXIT=101`，`FAILED. 11 passed; 8 failed` |
| C 只反转主体比较（`:204` `!=` → `==`） | `EXIT=101`，`FAILED. 11 passed; 8 failed` |

**失败集合逐条列出（不是凭印象，是从日志里 grep 出来的）：**

| 测试名 | A | B | C |
| --- | :-: | :-: | :-: |
| `cm04_a_profile_id_passed_as_a_session_handle_is_not_found` | | ✗ | ✗ |
| `cm05_a_cross_organization_owner_looks_identical_to_an_absent_session` | ✗ | | |
| `cm05_absent_and_foreign_handles_are_byte_identical` | ✗ | ✗ | ✗ |
| `cm05_cancelling_a_foreign_execution_never_reaches_the_driver` | ✗ | ✗ | ✗ |
| `cm06_a_forged_identity_in_the_body_cannot_override_the_principal` | ✗ | ✗ | ✗ |
| `cm06_a_job_owned_by_another_organization_is_refused` | | ✗ | |
| `cm06_a_job_owner_carries_no_principal_so_same_organization_peers_pass_the_gate` | | ✗ | |
| `cm06_a_peer_principal_in_the_same_organization_is_denied` | ✗ | | ✗ |
| `cm06_an_editor_owned_by_another_organization_is_refused` | ✗ | ✗ | ✗ |
| `cm06_naming_another_users_editor_is_refused` | ✗ | | ✗ |
| `cm06_the_real_owner_is_still_accepted` | | ✗ | ✗ |

三条集合**两两不同**：B 与 C 的差集正好是「job 两例 vs peer/命名两例」，
即组织比较只由 `OwnerRef::Job` 那一路独立承重、主体比较只由 `Editor` 那一路独立承重。
⇒ 上一版「失败集合相同 ⇒ 各自独立被覆盖」的推论**是错的**：集合相同根本不能推出独立，
集合不同才能。

**一个必须说清的副作用**：`cm04_...` 之所以在 B/C 下红，是因为它 `:484-490` 有个
「真句柄必须放行」的对照体，作者器一反转，对照体就红。它不是 CM-04 的语义被破坏，
而是**跨议题的交叉敏感**——所以「哪些测试红了」不能当「哪个议题被破坏」的判据，
只能当「这处代码有人依赖」的存在性证据。

#### 被推翻的旧结论（保留原文以便追溯，判定以本节为准）

1. 旧「变异2b 只反转主体比较 = `5 passed; 9 failed`」⇒ **实测 `11 passed; 8 failed`**。
   旧数字取自 14 例的旧套件，换套件后失效；**同一句话里的 9/5 本身也曾与旧套件的
   实测不符**，已作废。
2. 旧「变异3 只反转组织比较 ⇒ 失败集合与变异2 不同 ⇒ 各自独立被覆盖」
   ⇒ **推论方向错了**。当时的实测里变异 2 / 2b / 3 的失败集合**逐字相同**，
   「相同 ⇒ 独立」不成立（相同也可能是同一处代码同时驱动三者）。
   修复轮补了真正的跨组织 `Editor` 负例与独立命名后，B 与 C 才真的分家。
3. 旧「变异1 两处 `if` → `if false` = `5 passed; 9 failed`」⇒ **作废**，不再单独跑。
   变异 A（删调用点）比 `if false` 更能说明承重面，且失败集合更大。

### 接线守卫（`owner_binding/production_wiring.rs`，本轮新增）

| 变异 | 结果 |
| --- | --- |
| ~~往**生产**文件 `src/gateway/request.rs` 栽一处 `AlwaysAllow`~~ | **作废——这条从未真的跑过**，详见下方更正 |
| 往生产文件 `src/gateway/mod.rs` 的 `pub(crate) mod testing_support;` 之后插一行探针 | `EXIT=101`，`no_production_file_wires_an_authorizer_that_always_allows` 红，报 `packages/runtime/src/gateway/mod.rs:67` |
| 阳性对照：同一探针插到 `mod.rs` 文件末尾 | `EXIT=101`，报 `packages/runtime/src/gateway/mod.rs:800` |
| 只在注释 / 字符串字面量 / `#[cfg(test)]` 块 / `use` 里写 `AlwaysAllow` | `EXIT=0`，`the_wiring_guard_ignores_prose_and_test_only_mentions` 绿 |

**更正（R3）**：本节原先写「真实反例报 `request.rs:603`，行号与 `grep -n` 实测一致」。
**那行结论是伪造的**——`request.rs` 实测 **599 行**，`AlwaysAllow`/`AlwaysDeny`
**零命中**（`git grep` 退出码 1），603 行不存在，那次运行从未发生。当时真正跑过的
只有内存字符串上的合成反例，行号一致性并未在**真实文件**上验证过。
上表两条 `mod.rs` 探针才是磁盘上真跑过的记录：两次都报出了等于插入位置的行号，
这才是不丢换行这条不变式的活证。合成反例的缺陷也已定位并修掉——它往一个**没有**
`#[cfg(test)] mod x;` 无花括号声明的内存串里栽探针，因此从未复现真实文件上会被吞的窗口。

### 编译期负例（`request_cm06_negatives.rs` / `request.rs`）

> 本表**上一轮实测，本修复轮未重跑**（本轮没动 `request.rs` / `request_cm06_negatives.rs`，
> `--doc` 的 `7 passed; 0 failed` 在 `bf7c35b7` 上重新跑过）。表里的 `line 29 / 116 / 196`
> 是 rustdoc 报的 doctest 行号，不是文件行号。

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
| `invariants.rs` 的 `expected` 按文件性质分流：`owner_binding.rs` 进**生产文件**组（它 `pub mod` 进生产二进制，本就该受生产路径禁用词扫描），`request_cm06_negatives.rs` 进**测试专用**组并同列 `test_only`（它只编进 `#[cfg(doctest)]`，与该列表注释「测试专用」同类） | 往 `owner_binding.rs` 生产段栽 `panic!` ⇒ `EXIT=101`，守卫报 `owner_binding.rs:508 生产路径出现 panic!(：    panic!("x");`。★ **本轮重新实测过**：栽在第 508 行、报第 508 行，**行号保真**；上一版记的 `:235` 是旧版文件（19282 字节那版）上的行号，文件长到 505 行后它就失效了 |

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
- ★ **「上一层已经挡住」是错的，本轨不再这么写。** `identity_policy.rs` 的
  `check_owner`（**定义在 `:137`**，`ctx: &RequestContext, owner: &OwnerRef,
  authorized_job: Option<&JobId>`）实测**全仓没有任何生产调用点**：
  `git grep -in check_owner -- .` 的 16 处命中里，8 处是它自己文件内的单测
  （`:270`、`:276`、`:283`、`:284`、`:285`、`:292`、`:297`、`:307`），
  1 处是 `dto/requests.rs:183` 的文档链接，其余 7 处都是本轨自己写的**文档说明**
  （`owner_binding.rs` 3 处、`tests/owner_binding.rs` 2 处、本文件 1 处）。
  `src-tauri` 全树零命中。⇒ 「上层兜底」不存在。
- 而且就算它被调用也**补不上这个洞**：它比的是调用方**显式传入**的 `authorized_job`
  与 `owner.job_id`（`identity_policy.rs:165-171` 的 `OwnerRef::Job` 分支），
  那是「这个 job 有没有获授权」，不是「这是不是同一个人」；而它的 `OwnerRef`
  是 `datazen_platform_api::context::OwnerRef`（`packages/platform-api/src/context.rs:65`），
  **`Editor` / `Job` 两个变体里都没有 `organization_id` 与 `principal_id`**，
  结构上就看不到组织与主体。⇒ CM-06 的 job 半边在本层与在应用层都**合不上**，
  正确处置是记 **`PARTIAL`**，不是「已覆盖」。
- **归属：需要 `OwnerRef::Job` 增加 `principal_id`（或让 `Authorizer` 入参带被授权 job），
  属 connection 类型轨 / 网关契约轨。本轨不改。**
- 已把这条登记放进**合并后仍在**的地方：`src/gateway/owner_binding.rs` 模块头
  「未闭合项登记」（标题在 `:74`，其中「上层没有兜底」一段 `:82-88`）与 `identity_policy.rs` `check_owner` 上的
  `# 契约债（不要把本函数当成网关归属闸门的兜底）` 文档段（`:120`）。

### 2. 取消路径残留存在性预言机（**本轨判定为 WARN，登记不修**）

- 事实：`gateway/mod.rs:641-644` 的执行记录查表发生在 `:654` 的 `verify_binding`
  与 `:657` 的 `owned_view` **之前**。故「`executionId` 根本不存在」得
  `CancelFailed("unknownExecution")`（`api_code()` 为 `None`），「存在但不归我」得
  `sessionNotFound` —— 两个对外码可区分，攻击者据此判定该 `executionId` 是否存在。
- ★ **上一版这里列的五处引用，实测四处是错的**（照抄会让人去改钉死的门禁）：
  | 旧引用 | 实测 | 说明 |
  | --- | --- | --- |
  | `tests/registry_audit.rs:103` | `tests/registry_audit.rs:131` | 断言 `matches!(projection, ExitProjection::NotOnTheWire { .. })` |
  | `tests/registry_cancel.rs:140,289,311` | **该文件零命中** | `api_code()` / `NotOnTheWire` / `UNKNOWN_EXECUTION` 在该文件里一次都不出现，**整条删掉** |
  | `tests/p3_session_port_contract.rs:428,472` | `tests/p3_session_port_contract.rs:326,336` | 两处都断言 `assert_eq!(err.api_code(), None)` |
  | `tests/gateway_contract/cancel.rs:74,118` | `tests/gateway_contract/cancel.rs:127` | 断言 `assert_eq!(*reason, binding::UNKNOWN_EXECUTION)` |
  | `gateway/mod.rs:655`（`verify_binding`） | `gateway/mod.rs:654` | |
- 为何本轨不修：**门禁从三处独立钉死了当前语义**（上表），动它就要同时改这三处契约测试，
  而这三处属于别的轨；且与 `datazen-p3-cm60-pressure-drain` @ `eb607ad88`、
  `datazen-p3-cm70-fu2-token-leak` @ `f1d843271`、`ta-cm60` @ `eb607ad88` 共享。
- 本轨做到哪一步：拒绝时**驱动 `cancel` 调用次数 = 0** 已由
  `cm05_cancelling_a_foreign_execution_never_reaches_the_driver`（`tests/owner_binding.rs:558`）
  钉死——它**不是** `#[should_panic]`，而是同一个测试里正反对照：先断言 U1 取消 U2 的
  执行得到 `SessionNotFound` 且 `port.cancel_calls() == 0`（`:576-582`），
  再让**真 owner U2** 取消同一条执行，断言它撞到的是端口替身的 `InvariantBroken`
  而不是归属比较、且 `port.cancel_calls() == 1`（`:584-597`）。
  排除「因为根本没调用所以是 0」的空转绿。
- 严重度：上一版记为**未闭合缺陷**；本轮复核认为在「取消」这个动作面上，
  宿主侧语义由 `registry/port.rs` 契约与上述门禁共同固定，**改它属契约变更而非本轨修复**，
  故降为 **WARN** 并登记到 `tests/owner_binding.rs:55-61`（合并后随文件留存）。
- **归属：cancel 语义所属轨（建议 P7 或独立的 cancel 闸门轨）。**

### 3. CM-04 的 `executeAtTarget` 半边

- 事实：`execute_at_target` 是 **application 层契约，仓库内零实现**
  （`pub trait ConnectionUseCases` 声明在 `packages/application/src/sessions.rs:49`，
  方法声明在 `:102-106`；`impl ConnectionUseCases` 全仓**零实现**——
  `git grep -n "impl ConnectionUseCases"` 唯一的命中是
  `src-tauri/src/platform/adapter.rs:432`，而那一行是
  `no_shell_implementation_of_the_thirteen_use_cases_exists`（`:428`）里
  **断言它不存在**的 `assert!(!production.contains("impl ConnectionUseCases"))`，
  注释写明「实现就得编造 revision / epoch / 幂等值」。⇒ **这不是「没人查过」，
  是仓库自己有一条会红的门禁钉着它**，本轨直接引用该结论即可。
- runtime 侧的 `ExecuteAtTargetRequest`（`connection/session.rs:186-189`）是
  `{target, call, idempotency_key}`；其 application DTO 另需
  `expected_config_revision: Counter`（CAS，无 serde 默认值）。
- 故「向 `executeAtTarget` 传未知 profile ⇒ 不可见配置错误」在 H 层**没有可注入的
  调用点**，不编造。CM-04 的 `executeInSession` 半边已闭合并有门禁
  （`cm04_a_profile_id_passed_as_a_session_handle_is_not_found`）。
- **归属：application / 组装层。**

### 4. `registry/actor.rs` 的两处未文档化 `SessionClosed`

- 事实：`registry/actor.rs` 的 `exec<T>`（`:177`）与 `control<T>`（`:190`）都是同一副
  骨架：channel 发送失败返回 `UnknownSession`（有文档），而
  `rx.await.unwrap_or(Err(RuntimeError::SessionClosed(..)))` 出现在
  **`:185`（`exec`）与 `:198`（`control`）**。
  `registry/port.rs:56-59` 只对 `execute_in_session` 声明了 `SessionClosed`
  属于失败集合，**`exec` / `control` 这两条路径不在任何 trait 的失败集合里**。
- ★ 上一版写「**第二处**」，实测是**两处**（`:185` 与 `:198`，此前漏了 `control`），
  且旧引用 `port.rs:42-45` 是错的（实际：失败条件清单那段文档 `:56-58`，其中
     `SessionQuarantined` 出现在 `:58`；`execute_in_session` 的声明在 `:59`）。
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
   doc 注释起于 `:379`；经 `src/connection/mod.rs:33` 的 `pub use types::{...}`
   再导出；作为字段使用在 `src/connection/session.rs:155` 的 `pub owner: OwnerRef`。
   四个变体字段见 `types.rs:383-401`（`Job` 变体在 `:390-394`）。
   ★ **上一版这里写的 `packages/platform-api/src/context.rs:180` 是错的，
   实测 `:65`**（`pub enum OwnerRef`，4 个变体 `:66-86`）——`context.rs` 全文远不到 180 行。
   那个同名不同类型的 `OwnerRef` 正是 CM-06 job 半边合不上的根因，见上文第 1 节。
4. **CM-01「编译期负例」是先例**：核实为**散文级**——仓库里没有 compile-fail
   fixture，只有 `connection-management.md:867-871` 的文字。本轨按 CM-01 的**意图**
   实现（`compile_fail` doctest + 反序列化负例 + 源码结构守卫），不援引一个不存在的先例。
5. **「本轮零产出」不成立**：核查当时 `git status --porcelain` 为
   ` M packages/runtime/src/gateway/mod.rs`、` M packages/runtime/src/gateway/provenance.rs`、
   `?? packages/runtime/src/gateway/owner_binding.rs`（19282 字节）；单测结论行
   `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 436 filtered out`。
6. **`ApiErrorCode` 未从 `datazen_runtime::connection` 再导出**：`connection/mod.rs:22`
   只有 `pub use error::{ApiError, ProviderError, RuntimeError};`。集成测试须写
   `use datazen_platform_api::error::ApiErrorCode;`。
7. ★★ **「网关无生产组装根」——这条曾只活在台账里，合并即失效；现已落成两处可执行事实。**
   - 复核后的实测（全仓 `git grep`，`src-tauri` 逐条查过）：
     `OwnerMatchAuthorizer` 的 9 处命中全在 `src/gateway/owner_binding.rs`（定义与单测）、
     5 处在 `tests/owner_binding/production_wiring.rs`（守卫自身）、
     2 处在 `tests/owner_binding/support.rs`（夹具）——**生产目录零命中**；
     `ExecutionGateway::(new|with_submission_tokens)` 全仓 12 处命中，拆开是
     **8 处真实构造点**（`facade_support.rs:337`、`facade_tests.rs:121`、
     `gateway_fixtures/mod.rs:610,634,679,692,711`、`owner_binding/support.rs:316`）
     + 4 处散文/守卫夹具（`owner_binding.rs:53` 的模块头、`cm70_idempotency_replay.rs:17`、
     `production_wiring.rs:5`、`:466` 那段 `r#"…"#` 里的**合成反例**）。
     8 处真实构造点全在 `#[cfg(test)]` 模块（`gateway/mod.rs:74-77`）或 `tests/` 下，
     **`src-tauri` 零命中**。
     ⇒ 结论成立：**归属闸门在任何运行中的应用里都未生效**。
   - 落点一（要求随代码走）：`src/gateway/owner_binding.rs` 模块头
     「接线要求：生产组装必须显式传 `OwnerMatchAuthorizer::shared()`」（标题在 `:50`；
     模块头整体为 `:1-113`，`:115` 起是代码）。
   - 落点二（不靠自觉）：`tests/owner_binding/production_wiring.rs` 新增 5 条门禁，
     扫全仓 Rust 源码（剥注释/字符串/`#[cfg(test)]` 块/import），任何**生产**文件里
     出现 `AlwaysAllow` / `AlwaysDeny` 即红并指名文件与行号；带合成反例与
     「注释/字面量/测试块不算」的负对照，两者都实测过。
   - 今天扫描为空（空绿），**第一个生产接线者**就是触发它的人——这是有意的：
     门禁在缺陷发生的那一次才有话可说。
   - 缓解要说准：`authorizer` 是**必填位置参数且无 `Default`**，所以漏传是编译错误；
     唯一静默形态是「传了个恒放行的替身」，守卫对的正是后者。

## 本轮（修复轮）新增：合并后仍读得到的结论

上一版的勘误只写在台账里，合并删掉 `progress.md` 就全没了。本轮把每条
**结论**都改写进了代码或测试的文档注释，台账只留推导过程：

| 结论 | 合并后仍在的位置 |
| --- | --- |
| 生产接线必须显式传 `OwnerMatchAuthorizer::shared()` | `src/gateway/owner_binding.rs` 模块头标题 `:50`（整段 `:50-73`）+ `production_wiring.rs` 5 条门禁 |
| CM-06 的 job 半边 = **`PARTIAL`**（不是「上层挡住」） | `src/gateway/owner_binding.rs` 模块头 `:74`（其中「上层没有兜底」一段 `:82-88`）；`tests/owner_binding.rs:163-185` |
| 应用层 `check_owner` 无生产调用点、结构上也看不到 org/principal | `identity_policy.rs:120` 的 `# 契约债` 段 + `owner_binding.rs:82-88` |
| CM-05 六个接口的处置（执行 CLOSED / 取消 PARTIAL / 读取·关闭 N/A / 订阅·下载 P7） | `tests/owner_binding.rs:36` 的表（表体 `:38-52`） + `cm05_the_gateway_action_surface_is_exactly_execute_and_cancel` |
| 取消路径存在性预言机 = **WARN**，本轨不修 | `tests/owner_binding.rs:55-61` |
| 仓库里有**两个同名** `OwnerRef`，无交叉校验 | `src/gateway/owner_binding.rs` 模块头 `:95`（整段 `:95-113`）+ `identity_policy.rs:120` |
| 变异 A/B/C 的实测失败集合（承重面） | `tests/owner_binding.rs` 各测试的文档注释（本文件「变异证明」一节是原始记录） |

## R3（第 3 轮修复）：验收回 2 个 BLOCKER + 1 个 WARN

### BLOCKER A —— 伪造的 `request.rs:603` 反例

**性质**：不是代码缺陷，是**证据伪造**。四处引用（`owner_binding.rs` 模块头、
`production_wiring.rs` 模块头两处、本文件上一版 `:139` + `:142-144`）都声称
「在真实文件 `request.rs:603` 上跑出过红」。实测：`request.rs` **599 行**，
`AlwaysAllow`/`AlwaysDeny` **零命中**（`git grep` 退出码 1），603 行不存在。
四处已全部改写为**实际跑过的**记录，并显式声明那次运行从未发生。

★ 这条教训本身写进了代码：`production_wiring.rs` 模块头现在直接写着
「这里原先写着……**那是错的**……那次运行从未发生」。伪造结论删掉没用，
留一条**反伪造声明**才有约束力。

### BLOCKER D —— `#[cfg(test)] mod x;`（无花括号声明）被误吞生产代码

**性质**：守卫的真实盲点。旧实现把 `#[cfg(test)]` 属性之后**下一个 `{`**
当成内联块的开花括号，于是 `mod.rs:65-66` 的 `#[cfg(test)] pub(crate) mod
testing_support;` 会跟后面某个毫不相干的 `impl Foo {` 配对，中间整段生产代码被剥掉
（剥注释与字符串后的行号为 **64..87**，520 字节）。

**修法**：`inline_module_brace()` 要求 `{` 属于**同一条声明**——从 `mod` 关键字
往后扫，**先遇到 `;` 就判定为无花括号外部声明**、直接返回 `None`。这同时覆盖
`pub mod x;` / `pub(crate) mod x;` / `pub(in a::b) mod x {` / 裸 `mod x {`，以及
rustfmt 把 `{` 换行写的合法形态 `mod tests\n{`。

**新旧判别器全量对照**（在**剥掉注释与字符串**的文本上逐文件比对吞掉的区间）：

| 指标 | 实测值 |
| --- | --- |
| 受影响文件 | **54** 个 |
| 受影响区间 | 55 处，合计 324348 字节 |
| 这些区间里真正出现 `AlwaysAllow`/`AlwaysDeny` 的 | **0 处（全部 54 个文件 `always_mentions=0`）** |

⇒ D 是**潜伏的绕过通道，不是已在生效的漏洞**。最大的几处（`sync/exec.rs` 7..1018
32654B、`app_data_archive.rs` 532..1323、`sync/plans.rs` 1164..1883）都不在本轨。
本轨子集：`gateway/request.rs` 395..598、`gateway/mod.rs` 64..87（仅 520 字节）。

★ **更正上一版台账的两个说法**：早前记的「23 个文件 / `mod.rs` 95 行 / 3737 原始字节」
**作废**，权威数字是上表的 **54 个文件**；「盲区在 `gateway/mod.rs` 最大」**也是错的**——
`mod.rs:64..87` 只有 520 字节，远排在 `sync/exec.rs` 的 32654 字节之后。之所以仍要修，
是因为守卫的**不变量**（剥除不得吞掉生产代码）本身被破了，与眼下是否正好有人违规无关。

**常驻回归**（3 条，全部不靠人记）：
1. `a_braceless_test_module_declaration_must_not_swallow_the_next_block` —— 无花括号
   夹具必须只命中 1 处且在第 6 行；反向夹具（真 `mod inner_tests { … }`）必须命中
   `vec![8, 9]`，证明没有矫枉过正。
2. `the_wiring_guard_reads_the_real_gateway_module_past_its_test_declarations` ——
   **从磁盘读真实的 `gateway/mod.rs`**，断言它今天干净；把探针分别种在
   (A) 第一个 `#[cfg(test)]` 偏移 +1（过去被吞的那个窗口）与 (B) 文件末尾（阳性对照），
   两处都必须在**种进去的那一行**被抓到。
3. `no_braceless_test_module_declaration_is_ever_swallowed` —— 全生产文件走查，
   任何 `inline_module_brace` 返回 `None` 的 cfg-test 声明即红。**并断言输入非空**：
   `scanned > 100`、`外部测试模块声明 ≥ 20 处 / ≥ 10 个文件`，实测输出
   「外部测试模块声明：289 处 / 136 个文件，扫描生产文件 698 个，全部未被误吞」。

### WARN B —— 本轨测试文件不在 800 行门禁里

`gateway_contract/invariants.rs` 的 800 行断言（实测在 `:229` 与 `:320`）原先只覆盖
`tests/gateway_contract.rs` + `tests/cm70_idempotency_replay.rs` + `tests/cm70/*.rs` +
`tests/gateway_contract/*.rs`，**`tests/owner_binding.rs` 与 `tests/owner_binding/*.rs`
在任何一个清单里都没有**。已补：硬编码清单加 `tests/owner_binding.rs`，并新增
**整个 `tests/owner_binding/` 目录扫描**（并断言该目录扫出来非空，防止扫描悄悄失效）。

**负对照（证明这条新覆盖不是空绿）**：把 `tests/owner_binding/support.rs` 补到 802 行
⇒ `EXIT=101`，门禁报 `tests/owner_binding/support.rs 超过 800 行`；随后
`git checkout --` 还原并 `touch`，`cmp` 验证与备份**逐字节相同**。
补覆盖后本轨自己的 `production_wiring.rs` 曾压到 **799 行**（`cargo fmt` 后复核）——
**距自己刚加的 800 行门禁只剩 1 行余量**，这是一次「刚好塞进去」的脆弱结构，
不是「修好了规模问题」。本轮已按职责拆分（见下），不再靠压行数硬塞。

**已登记但未纳入**：`tests/cm70_no_disk.rs` 实测 **807 行**，且此前**没有任何** 800 行
门禁覆盖它（它自己 807 行却不在自己的门禁清单里）。补进去会立刻变红，拆分属独立改动。
**这条写进了 `invariants.rs` 的代码注释**，不是只写在台账里——台账随合并删除后缺口不消失。

### 空绿这件事，说准

`no_production_file_wires_an_authorizer_that_always_allows` 证明的是
「**第一个把 `AlwaysAllow` 接进生产路径的人会红**」，**不是**「今天的接线是对的」。
后者靠的是 `owner_binding.rs` 的行为测试，不是这条扫描。这个区分已写进
`production_wiring.rs` 模块头。输入非空由第 3 条不变式测试的
`289 处 / 136 个文件 / 698 个扫描文件` 断言兜底。

### 本轮最后一步：按职责拆文件

`production_wiring.rs` 加完 BLOCKER D 的修复与三条回归测试后到 **804 行**，越过仓库
800 行红线（`cargo fmt` 之前）。第一反应是压缩散文把它塞回去——**那是在给下一个人
留同样的坑**，所以改成按职责拆：

* 新增 `tests/owner_binding/wiring_shape.rs`（**231 行**）：只放「剥除器模块形状」
  的不变式测试——`no_braceless_test_module_declaration_is_ever_swallowed`、
  `a_braceless_test_module_declaration_must_not_swallow_the_next_block`、
  `the_wiring_guard_reads_the_real_gateway_module_past_its_test_declarations`
  及两个私有辅助。
* `production_wiring.rs` 降到 **589 行**，只保留「生产路径有没有接上 `AlwaysAllow`」。
  供兄弟模块复用的 9 个函数改为 `pub(super)`，**没有复制实现**——
  复制会让两份各自腐化，正是这轮要根治的病。
* 拆分线本身是职责线不是行数线：`production_wiring` 回答「量没量到接线」，
  `wiring_shape` 回答「量的是不是完整那一段」。BLOCKER D 正落在这条缝上。

**拆分是行为保持的实测结论**，不是推断：拆分前后 `289 处 / 136 个文件 / 698 个扫描文件`
三个数**逐字相同**（`--nocapture`），`running 22 tests` 也未变；三个测试名由
`production_wiring::*` 变为 `wiring_shape::*`，**没有增删任何断言**。
新文件落在 `tests/owner_binding/` 目录下，因此被 WARN B 修好的整目录扫描自动覆盖，
不需要再往 `invariants.rs` 的清单里加一行。

### FAIL-4 复评：**未关闭**

要求「归属闸门在任何运行中的应用里生效」并没有因为 D 被修掉而满足——
今天它仍然只在 `#[cfg(test)]` 模块与 `tests/` 下有构造点，`src-tauri` 零命中。
D 的修复只是让**门禁本身**不再有盲区，不是让闸门生效。状态不变。

### R3 门禁实测（最终一次，各跑一次）

拆分文件后**必须整组重跑**，不能只重跑受影响的两条——文件拆分会改测试名、改编译单元，
声称「拆分只影响排版」是不成立的猜测。本表是拆分**之后**的整组实测，
首尾 HEAD 均为 `664ec3f03`（未变），porcelain 首尾**同为 6 个条目**（4 改 + 1 增 + 台账）。

| 命令 | 结论行（逐字） | EXIT |
| --- | --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | （输出 0 行） | 0 |
| `cargo test -p datazen-runtime --lib` | `running 441 tests` / `test result: ok. 441 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s` | 0 |
| `cargo test -p datazen-runtime --test owner_binding` | `running 22 tests` / `test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.92s` | 0 |
| `cargo test -p datazen-runtime --test cm70_no_disk` | `running 13 tests` / `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` | 0 |
| `cargo test -p datazen-runtime --test gateway_contract` | `running 51 tests` / `test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` | 0 |
| `cargo test -p datazen-runtime --doc` | `running 7 tests` / `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s` | 0 |
| `cargo test -p datazen-runtime` | 24 个目标，`running` 合计 **718**、`passed` 合计 **718**、`failed` 合计 **0**、非 `ok.` 结论行 **0** | 0 |
| `./node_modules/.bin/tsc --noEmit` | （输出 0 行） | 0 |

**非空绿证明**：逐条 `running N>0`——441 / 22 / 13 / 51 / 7，全量 718；
`owner_binding` 22 条里三条拆分后的 `wiring_shape::*` 逐条 `ok`。
`fmt` 与 `tsc` 本就不打印 `running`，它们的成功信号是「输出 0 行 + EXIT=0」。

`owner_binding` 由 19 变 **22**、整 crate 由 715 变 **718**，差的正是本轮新增的 3 条
回归测试；这是预期增量，未为对齐旧数字而改动任何断言。仓库 flake 本轮 0 次复现。

## 已知环境问题（非本轨）

- 仓库根 `cargo fmt` 必失败：gitignored 的 `src-tauri/src/driver_init.rs` 在每棵
  工作树里都缺失。故门禁一律用 `cargo fmt -p datazen-runtime -- --check`。
- 工作树 `node_modules` 是指向主仓的符号链接，pnpm 的依赖预校验会拒绝
  `workspace hoist directory is not a real directory`；用
  `pnpm --config.verify-deps-before-run=false typecheck` 绕过。

---

合并进 main 前必须删除本文件。