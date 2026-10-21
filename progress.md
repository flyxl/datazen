# F1 轨台账：data-transfer 计划存储端口化

> 分支 `feature/migration-store-ports`，基线 `65dac1e2c`。
> 本文件是开发期台账，合并后必须删除。

## 范围

只做 data-transfer 这一半：`packages/data-transfer/src/plan_store/` 的进程级全局存储改成
端口 + 宿主注入的实例。未触碰 `packages/data-sync/**`、`packages/platform-api/**`、
`packages/feature-core/**`、`src-tauri/src/commands/sync/**`、其它 worktree；未 push。

## 三个提交

| # | commit | 内容 |
| --- | --- | --- |
| ① | `0accfda7d` | 包内：`TransferPlanStorePort` 端口 + `InMemoryTransferPlanStore`（HashMap 实现留在包内）+ 去掉 `static STORE`；`TransferCheckpointSession` 改为持有注入的端口 + 作用域 |
| ② | `d3306272f` | 宿主：`AppState.transfer_plans: Arc<dyn TransferPlanStorePort>`，组装期创建并注入；调用点传 `OrganizationScope::from_context(&ctx)`；新增 `commands/data_transfer/plan_scope.rs` |
| ③ | `d8894d158` | 孤儿测试接入（`plan_store/tests.rs` 首次被编译）+ `docs/architecture/backend/data-transfer.md` 同步 |

> **注意**：① 只改包内，单独 checkout ① 时宿主 crate **不能编译**（包 API 已变）。这是三步拆分的
> 预期结果：①②③ 合起来才是一个可编译的树。合流时按顺序应用 ①②③。

## 关键设计决定

1. **端口在包内，规则也在包内**：配对、指纹、一次性消费、恢复语义、commit boundary 一律留在
   `datazen-data-transfer`；宿主只决定"哪个实例"，不重写规则。HashMap 实现也留在包内
   （`InMemoryTransferPlanStore`），宿主不搬实现。
2. **作用域用 platform-api 的 `OrganizationScope`**，不自造类型。越权访问按"不存在"处理：
   跨组织的 plan 与未知 id 返回**完全相同**的错误文案（CM-05），端口层不产生"无权限"错误。
3. **桌面只承认一个组织**：`LOCAL_ORGANIZATION_ID`（`src-tauri/src/platform/identity.rs:36`）。
   作用域经 `plan_scope::desktop_organization_scope()` → `OrganizationScope::from_context(&ctx)`
   取得；`ctx` 由宿主按桌面身份组装，不来自请求体（INV-01）。
   **刻意不依赖 `platform.require()`**：桌面组织 id 是进程常量，若依赖 adapter 就绪，
   `env -u USER` 下计划存储会整体不可达（会新增一批失败）。实测无新增失败（见下）。
4. **端口是同步 trait**：claim 是单次原子 `Available → Executing`，`async` 签名会让无关的
   `await` 卡在检查与写入之间。

## 孤儿测试激活结果

- 接法：**保留独立文件 `plan_store/tests.rs`，在 `plan_store/mod.rs` 加 `#[cfg(test)] mod tests;`**。
  理由：内联进 `mod.rs` 会让 mod.rs 变成 ~1600 行（远超 800 行上限），且该文件原本就是独立文件，
  改动最小。
- **实际测试数是 19 条，不是任务书里说的 40 条**（`grep -c '#\[test\]'` = 19）。
- **第一次真正编译就炸了**（这才是"从未运行"的真实代价，全部是**编译期**缺陷，不是断言失败）：
  - `Some(crate { ... })` —— 少了 `::SqlFileTarget`，无法解析；
  - `kind: crate::Table` —— `crate` 根没有导出 `Table`，应为 `crate::DdlPreviewKind::Table`；
  - `Some(crate::Gzip)` —— 应为 `crate::SqlFileCompression::Gzip`；
  - 没有 import 恢复钩子 trait（`crate::resume::TransferResumeCheckpoint`），session 的
    `prepare_table / advance_table / token / is_invalidated` 全部找不到。
  这些是"让文件能编译"的修复，**没有改动任何断言**。
- 断言计数：改动前 76 条（48 `assert!` + 18 `assert_eq` + 10 `expect` 上下文），改动后 81 条
  （52 + 19 + …），差额 **+5 全部来自本次新增的跨组织测试**。逐条比对无删除、无削弱。
- **19 条孤儿测试第一次真正执行，全部通过（0 红）**，没有暴露既有逻辑缺陷。

## 新增测试（CM-05）

`a_plan_of_another_organization_is_not_found_rather_than_forbidden`：
跨组织读 plan 的错误文案与读未知 id **逐字相同**，`peek_any / claim / create_checkpoint`
同样按"不存在"处理；拥有者自己仍然读得到自己的 plan。

## 门禁实测（逐字结论行）

工作树 `.worktrees/datazen-migration-store-ports`，`CARGO_TARGET_DIR` 指向本工作树 target。

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 无输出 |
| `cargo clippy -p datazen-data-transfer -p datazen --all-targets` | 0 | 无 error（437 条 warning 全是基线已有的） |
| `cargo check -p datazen-data-transfer -p datazen --all-targets` | 0 | `Finished` |
| `cargo check -p datazen --features webdriver --all-targets` | 0 | `Finished` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-data-transfer --lib` | 0 | `test result: ok. 246 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | `test result: ok. 1268 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out` |
| `env -u USER -u DATAZEN_DESKTOP_USER … cargo test -p datazen --lib` | 101 | `test result: FAILED. 1026 passed; 242 failed; 9 ignored`（**与基线逐条同名单，零新增**） |

### 基线对照（同机器、同 target、同一命令）

| 指标 | 基线 `65dac1e2c` | 本分支 HEAD |
| --- | --- | --- |
| data-transfer `--lib` | 226 passed | **246 passed**（+19 孤儿 +1 CM-05） |
| host `--lib` | 1266 passed | **1268 passed**（+2 `plan_scope` 单测） |
| host `--lib`（`env -u USER`） | 1024 passed / **242 failed** | 1026 passed / **242 failed**（失败名单 `comm` 比对：**零新增、零修复**） |

`env -u USER` 的 242 条失败清单已存 `/tmp/dz_base_failures.txt` 与 `/tmp/dz_final_failures.txt`，
两文件 `comm -13` / `comm -23` 均为空 → 集合完全相同。其中 data_transfer 相关 39 条，
AI/connection/data/backup 相关其余条目均为基线自身的 adapter 依赖失败。

已知 flake 未复现：`native_desktop_output_overflow_is_bounded_killed_and_reaped`、
`platform::live_source::tests::*`。

## 行数对照（验证"没搬家"）

| 范围 | 基线 | 现在 | 差 |
| --- | --- | --- | --- |
| `packages/data-transfer/src/**` 合计 | 25,889 | **26,274** | **+385** |
| `plan_store/mod.rs` | 541 | 206 | −335 |
| `plan_store/checkpoint.rs` | 639 | 255 | −384 |
| `plan_store/memory.rs`（新） | — | 713 | +713 |
| `plan_store/port.rs`（新） | — | 234 | +234 |
| `plan_store/tests.rs` | 1,042 | 1,199 | +157 |
| `src-tauri/**` 本轨改动 | — | — | +247 / −78（净 +169） |

包内代码**没有**被搬进宿主：包反而多了 385 行（端口契约 234 行 + 作用域贯穿 + 新测试），
宿主只多了 169 行净增量（AppState 字段 + `plan_scope.rs` 62 行 + 调用点穿参）。

## 遗留与待裁定

1. **宿主内有两个组织 id 常量**：计划存储用 `LOCAL_ORGANIZATION_ID`（`datazen-desktop-local`），
   而 `commands/data_transfer/job_api/runtime.rs::request_context()` 用的是 `datazen-local`。
   二者是两套独立命名空间（Job 仓储 vs 计划存储），当前不冲突，但同一形态里"一个组织两个 id"
   是隐患。本轨按任务书要求以 `LOCAL_ORGANIZATION_ID` 为权威源，**未**改动 Job 身份
   （会牵动 Job 仓储的既有数据，超出本轨范围）。
2. **端口 trait 有 17 个方法**（协调者/Tester 实测：`port.rs` 的 trait 方法 17 个；第 18 行命中是
   `PlanIssueRequest::new`，非 trait 方法），一次性覆盖 plan + checkpoint 两侧。
   **P0-5 裁定（协调者已接受）：17 个方法不拆。** 理由：platform-api §4.2 的「按聚合拆、
   不合并 trait」只管 platform-api 端口族（`#[async_trait]` + `PortError` + platform ID newtype），
   `TransferPlanStorePort` 是**领域包端口**（无 `#[async_trait]`、返回 `TransferError`、裸 `String`
   id）→ 不在所辖族内；且规则目的是「免得实现方被迫实现**不相关**方法」，而这 17 个是**同一
   聚合根**（计划 + 执行租约 + 恢复检查点）上的操作，拆开后持久化实现**仍必须同时实现两个**，
   还会拆散当前由一对 `Mutex` 提供的**单写者原子性**。该理由已写进 `plan_store/port.rs` 的
   模块头，防后续有人「按约定」顺手拆开。
3. `docs/architecture/backend/services.md` 未提及计划存储，**无需改动**（已核对）。

---

# 第 2 轮（修-测循环）：Tester 覆盖补强 + 三项小修

Tester 的测试提交 `aa0147bfd` 已由协调者 cherry-pick 进本分支（`f866f5b1f`，纯测试 +220/−0）。
它补上了**检查点侧**的跨组织覆盖：计划侧的隔离此前只被断言了一半，去掉
`peek_checkpoint` / `claim_checkpoint` 的归属检查不会让任何测试变红，写侧
（`mark_consumed` / `finish_execution` / `update_checkpoint` / `invalidate_checkpoint` /
`renew_execution` / `prepare_checkpoint_table`）更是**完全没有护栏**。新增 3 条用例：
`a_checkpoint_of_another_organization_is_not_found_rather_than_forbidden`、
`a_foreign_organization_cannot_mutate_the_owners_plan_or_checkpoint`、
`the_desktop_scope_is_the_only_organization_the_plan_registry_sees`（宿主侧，**无 adapter 依赖**，
刻意不引入 `env -u USER` 失败）。

## 第 2 轮的两个提交

| # | commit | 内容 |
| --- | --- | --- |
| ⑤ | `2f88305ad` | D3（5 条 clippy）+ D4（作用域复用，不再每次访问分配 UUID）+ `port.rs` 模块头补「不拆 trait」的理由 |
| ⑥ | `866c4e543` | D1（文档措辞收窄）+ `plan_store/mod.rs` 模块头同步 + 台账方法数更正 |

## D1 / D3 / D4 逐条处置

**D1（文档字面为假）**：原文断言「包内没有进程级全局单例」，而包内仍有前置存在的
`sql_file.rs:36` `static PATHS`（SQL 文件路径登记表）以及两个测试期全局。已把断言收窄到
**计划存储**，并点名其余登记：

> `packages/data-transfer/src/plan_store/` 定义计划存储端口 `TransferPlanStorePort`，包内实现 `InMemoryTransferPlanStore`（进程内 Mutex + HashMap）。宿主在组装期创建一个实例注入 `AppState.transfer_plans`，调用点按组织作用域访问；**计划存储没有进程级全局单例**（包的其它部分另有各自的进程级登记，例如 SQL 文件路径表 `packages/data-transfer/src/sql_file.rs` 的 `PATHS`）。计划冻结原始 job、驱动协议、结构/范围/映射指纹、预期 DDL 和目标只读状态，并带有效期。存储本身不持久化。

`plan_store/mod.rs` 的模块头同步收窄为「The **plan store** has no process-wide global any more」。
旧的 `src-tauri/src/commands/data_transfer/plans.rs` 引用（该路径从未存在）保持删除状态 ✅。

**D3（5 条 clippy）**：4× `needless_borrow`（`plan_store/tests.rs` 的 `tester_claimed_chunk` 里
`store`/`scope` 本来就是引用）与 1× `nonminimal_bool`（`plan_store/memory.rs`
`!x.is_some_and(|u| u > now)` → `x.is_none_or(|u| u <= now)`，**同一谓词**）已全部清除。
`cargo clippy -p datazen-data-transfer -p datazen --all-targets` 后，本轨文件
（`plan_store/**`、`plan_scope.rs`）**零 clippy 命中**（`grep -c plan_store` = 0，`grep -c plan_scope` = 0）。

**D4（每访问一次分配一个 UUID）**：作用域原本每次调用都新建 `RequestContext`（内含
`RequestId::new(format!("transfer-plan-{}", uuid::Uuid::new_v4()))`）。改为**进程内组装一次并共享**：

```rust
/// The local request id of the plan registry's request context.
///
/// **This id carries no meaning here.** The scope only ever names the
/// organization; the request id is not an authorization input, is not written
/// anywhere, and is never compared. It therefore gets a fixed local value
/// instead of a fresh UUID per access — the plan registry is read on every
/// plan access, and a UUID per read would be an allocation for nothing.
const LOCAL_DESKTOP_REQUEST_ID: &str = "transfer-plan-desktop";

/// The one request context the plan registry's scope is derived from.
///
/// The desktop is single-tenant, so the context is assembled **once** and
/// reused for the lifetime of the process.
fn desktop_context() -> &'static RequestContext {
    static CONTEXT: OnceLock<RequestContext> = OnceLock::new();
    CONTEXT.get_or_init(|| {
        RequestContext::new(
            OrganizationId::new(LOCAL_ORGANIZATION_ID),
            PrincipalId::new(LOCAL_DESKTOP_PRINCIPAL),
            None,
            ClientInstanceId::new(LOCAL_DESKTOP_CLIENT_INSTANCE),
            RequestId::new(LOCAL_DESKTOP_REQUEST_ID),
            None,
        )
    })
}

/// The organization scope of the plan registry.
///
/// Returned as a shared reference so that a plan access allocates nothing: the
/// scope is a constant of the desktop shape, so there is no reason to rebuild
/// it per call.
pub(crate) fn desktop_organization_scope() -> &'static OrganizationScope {
    static SCOPE: OnceLock<OrganizationScope> = OnceLock::new();
    SCOPE.get_or_init(|| OrganizationScope::from_context(desktop_context()))
}
```

调用点随之由 `&plan_scope::desktop_organization_scope()` 改为 `plan_scope::desktop_organization_scope()`
（返回值已是 `&'static OrganizationScope`），**一次计划访问现在零分配**。
新增单测 `the_scope_is_reused_so_a_plan_access_allocates_nothing`（断言两次取到同一实例）。

**端口不拆的理由（模块头）**：`plan_store/port.rs` 模块头新增一节，写明检查点侧与计划侧
**是同一聚合**——`create_checkpoint` / `prepare_checkpoint_table` / `advance_checkpoint_table` /
`claim_checkpoint` / `update_checkpoint(finished)` / `renew_execution` 都会在**同一次调用**里
同时改写计划状态与检查点状态；拆 trait 只会把单写者原子性的边界挪到 trait 边界上，
持久化实现仍然必须两侧都实现。

## 第 2 轮门禁（HEAD `866c4e543`，工作区干净，首尾各记一次 sha 均为该值）

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 无输出 |
| `cargo clippy -p datazen-data-transfer -p datazen --all-targets` | 0 | 0 error / 431 warning（全为基线既有；本轨文件 0 命中） |
| `cargo check -p datazen-data-transfer -p datazen --all-targets` | 0 | `Finished` |
| `cargo check -p datazen --features webdriver --all-targets` | 0 | `Finished` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-data-transfer --lib` | 0 | `test result: ok. 248 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | `test result: ok. 1270 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out` |
| `env -u USER … cargo test -p datazen --lib` | 101 | `test result: FAILED. 1028 passed; 242 failed; 9 ignored` |

`env -u USER` 一条**连跑三次**：第 1 次 243 failed，多出的唯一一条是**已知 flake**
`platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped`
（断言 `left: TimedOut` / `right: OutputLimit`，高负载下的超时，与计划存储无关，
在 `src-tauri/src/platform/native_tool/tests.rs`）；第 2、3 次均 **242 failed**，
`comm` 与基线 **零新增零消失**。常规 `cargo test -p datazen --lib` 一条里该 flake 通过。

测试计数：data-transfer **248**（与协调者基线一致，含 Tester 的 3 条新用例）；
host **1270**（协调者基线 1269 + 本轮 D4 新增的 1 条作用域复用单测）。

