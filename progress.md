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
2. **端口 trait 有 16 个方法**，一次性覆盖 plan + checkpoint 两侧。后续若要接持久化实现，
   拆成 `TransferPlanStorePort` / `TransferCheckpointStorePort` 两个 trait 更贴合
   platform-api "按聚合拆、不合并 trait"的约定；当前未拆是为了让宿主只注入一个实例。
3. `docs/architecture/backend/services.md` 未提及计划存储，**无需改动**（已核对）。
