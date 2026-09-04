# Deep-Review Remediation — 实施计划

> **来源**：2026-09-03 全库深度 Review（5 路并行：后端 Rust / 前端 React / 驱动插件体系 / 安全 / 质量工程 + 主线程取证）。
> **Playbook**：`docs/development/subagent-dev-playbook.md` + `docs/development/subagent/`（Coordinator/Coder/Tester/Rescuer）。
> **集成分支**：`feat/deep-review-remediation`（各轨 `feature/rem-*` 合入此分支）。
> **协调总览**：`docs/development/coordination/hub.md`（只读生成物，`node scripts/aggregate-hub.mjs`）。
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`（各轨仅维护本轨 `progress.md` + `bugs.md`）。

分轨依据是**文件冲突面**；每轨独立 worktree（`.worktrees/datazen-<track>`）；编码与测试分离；E2E 仅登记、R 阶段统一回归。

---

## 0. 角色与基线约定

| 角色 | 约束 |
|------|------|
| 协调者 | 不写业务代码；维护 hub；merge / worktree 清理；写锁台账；关键节点向用户同步 |
| 编码代理 | 仅在 `.worktrees/datazen-<track>`；`scripts/new-feature-worktree.sh <track> feat/deep-review-remediation`；全新实例 |
| 测试代理 | 全新实例（禁复用 Coder）、独立 worktree；只测不修；零信任；覆盖率 ≥80% |
| 基线 | 所有轨道 base = `feat/deep-review-remediation` |

**自检（每个代理 BOOTSTRAP 必报）**：`pwd`、`git rev-parse --show-toplevel`、`git branch --show-current`、`git status --short`。路径必须为分配的 worktree；分支不得为 `main`。

---

## 1. 波次编排

### Wave 1（8 轨并行：写路径互斥）

| Track | Review 结论 | 任务摘要 | 主要写路径 |
|-------|------------|----------|------------|
| **rem-sql-guard** | P0-1/H1/H2/M8/M7 | SQL Guard 加固：NFKC 全角归一、`\0` 拒绝、注释剥离后重分类、反斜杠转义、MCP permission 同步 | `src-tauri/src/sql_guard.rs`、`src-tauri/src/mcp/permission.rs` |
| **rem-key-import** | H3 | 备份导入 `.key` 覆盖防护：警告+拒绝覆盖（legacy 改 opt-in） | `src-tauri/src/app_data_archive.rs` |
| **rem-ipc-redact** | M9/B8 | IPC 错误脱敏：返回路径统一 `redact_secrets_for_log`，修正 `error.rs` 明文断言测试 | `src-tauri/src/commands/error.rs`、`src-tauri/src/workflow/command_runtime.rs`、`src-tauri/src/services/db_tools.rs` |
| **rem-scheduler** | P0-2 | WorkflowScheduler `in_flight` panic 泄漏：DropGuard/catch_unwind 保证 remove | `src-tauri/src/workflow/scheduler.rs` |
| **rem-panic-locks** | B1/B2/B3/B7 | 锁毒化与 block_on 治理：extensions RwLock、tray/monitor block_on→spawn、ssh mutex、deploy/store/connection unwrap | `src-tauri/src/extensions/mod.rs`、`tray.rs`、`monitor/`、`ssh_tunnel.rs`、`schema_diff/deploy.rs`、`store/mod.rs`、`commands/connection.rs`、`cache/schema_cache.rs` |
| **rem-ci-guards** | P0-3 | CI 接入三守护：`test:ids` + `test:ci-docs` + `i18n-sync-check`（ci.yml + ci-local.sh） | `.github/workflows/ci.yml`、`scripts/ci-local.sh`（缺口翻译键补英文占位除外——仅接脚本） |
| **rem-driver-contracts** | 契约违反×3 | MongoDB/ES/HBase `command_definitions()` 收敛到 `execute_command()` 实际处理集 | `packages/drivers/{mongodb,elasticsearch,hbase}/src/*.rs` |
| **rem-host-decouple** | R4/R5/R6 | Host 去驱动化：`mongodbFind.ts` 移入 Mongo 驱动包、`redis-db`→`isKeyValue`、redis_flush_gate 收敛、流行度排序驱动化 | `src/lib/mongodbFind.ts`、`src/windows/connection/*`、`src/stores/panelStore.ts`、`src/lib/databaseTypes.ts`、`src-tauri/src/redis_flush_gate.rs` |

**Wave 1 合并策略**：任一轨 Tester 闭环（TEST_DONE + bugs 全关）即可合入集成分支；合并后跑 `npx tsc --noEmit` + `cargo test -p datazen --lib` 健全性检查。

### Wave 2（4 轨：依赖 Wave 1 合并后的稳定树 / 大重构）

| Track | Review 结论 | 任务摘要 | 主要写路径 | 依赖 |
|-------|------------|----------|------------|------|
| **rem-sync-taxonomy** | R1 | sync category/family 下沉 `driver-api` trait + `DatabaseTypeMeta`，删除前后端 6 处重复硬编码 | `packages/driver-api/src/*.rs`、`src-tauri/src/transfer/pairing.rs`、`src/lib/syncPairing.ts`、`src/lib/databaseMeta.ts` | Wave1 合并（trait 改动影响面大） |
| **rem-ddl-atomicity** | R3 | `ddl_atomicity()` 改 trait 方法，各驱动自报 | `src-tauri/src/services/transaction.rs` + 各驱动 crate | Wave1 合并 |
| **rem-frontend-split** | F1–F8 | 大 store/大组件拆分 + VirtualBody memo + Workflow 类型守卫 + WorkflowChatPanel 运行时校验 + bridge targetOrigin 注释 + dead branch/console 清理 | `src/stores/*`、`src/windows/*`、`src/components/*`、`src/lib/extensionBridge.ts` | rem-host-decouple 合并（ConnectionPage 冲突面） |
| **rem-followups** | P2/P3 + M2/M4/M6/M10/M12/L | resolve-drivers 单测、版本一致性守护、WorkflowError 枚举化、save_settings 原子化、MCP 子进程 allowlist、插件权限分层、CSP 收紧、Windows ACL、allowlist 默认 deny-all、testing.md/e2e-coverage 修正 | `scripts/__tests__/`、工作流/脚本/文档/MCP 相关 | Wave1 合并 |

### R 阶段（全部轨道 MERGED 后，全量回归）

见 §6。

---

## 2. 落点侦察（协调者预填；代理须自行核实，行号以实施时为准）

### rem-sql-guard
- `src-tauri/src/sql_guard.rs`：`WRITE_VERBS` L5-10、`is_write_verb` L61-62、`json_to_sql_literal` L99-107、`split_statements` L186-224；自证旁路测试 L483-531。
- `src-tauri/src/mcp/permission.rs`：`sql_main_keyword`（与 guard 同族分类器）、`check_sql_allowed`。
- **验收**：`ＤＲＯＰ`/`DROP\0TABLE`/`DROP/**/TABLE`/`/* DROP */ TABLE x` 在 safe_mode+read_only 下全部拦截；反斜杠在字面量中转义；`cargo test -p datazen --lib sql_guard` + `permission` 全绿；原有合法 SQL 不误伤。

### rem-key-import
- `src-tauri/src/app_data_archive.rs`：导出排除 L84（`has_key_component`）、导入未排除 L94-97、覆盖点 L399（`zip_has_key`）。
- **验收**：zip 含 `.key` 时默认拒绝覆盖已存在密钥并返回明确错误；legacy 兼容改为显式 opt-in 参数；单测覆盖：含 key 的 zip 导入被拒、无 key 导入正常、opt-in 路径可用。

### rem-ipc-redact
- `src-tauri/src/commands/error.rs`：`CmdExt::cmd_err` L182-191（日志脱敏但 IPC 原文）、测试 L291-298（断言 IPC 含 `s3cret`——必须改为脱敏后断言）。
- `src-tauri/src/workflow/command_runtime.rs:35`、`src-tauri/src/services/db_tools.rs:33`（`format!("Failed to connect ... {e}")` 直传）。
- **验收**：所有 IPC 返回错误经 `redact_secrets_for_log`；含 `mysql://root:s3cret@...` 的错误到前端时无明文密码；既有测试更新后全绿。

### rem-scheduler
- `src-tauri/src/workflow/scheduler.rs:135-170`（insert 后 spawn，remove 仅正常路径）。
- **验收**：spawn 体用 DropGuard/`catch_unwind`/finally 语义保证 `in_flight.remove`；单测模拟执行 panic 后该 workflow 仍可再次触发；既有 scheduler 测试全绿。

### rem-panic-locks
- `src-tauri/src/extensions/mod.rs:91,216`（`.expect("extension registry poisoned")`）；`tray.rs:29,139` + 注释 L136；`monitor/engine.rs:123,127,232,247,254,284,324,340`；`ssh_tunnel.rs:49,56,79`；`schema_diff/deploy.rs:261,339,343,428,442`；`store/mod.rs:303-320`（read-then-write）；`commands/connection.rs:70-73`（`let _ =`）；`cache/schema_cache.rs:113-118`（读锁内 I/O）；`monitor/connections.rs:54`；`monitor/channels.rs:91`。
- **验收**：生产路径零裸 `unwrap/expect`（`#[cfg(test)]` 与静态初始化 `Regex::new().expect` 加注释除外）；poison 用 `map_err`/`into_inner` 恢复；`block_on` 移出同步回调；行为不变，lib 测试全绿。

### rem-ci-guards
- `.github/workflows/ci.yml`（118 行单 job，未调三守护）；`scripts/ci-local.sh`；`package.json` 中 `test:ids`/`test:ci-docs` 脚本；`scripts/i18n-sync-check.mjs`（实测 6 语言各缺 17 key）。
- **验收**：ci.yml 新增三 step（ID 术语 / CI-docs 一致性 / i18n-sync，其中 i18n 允许以 warning 先行——以不阻塞既有红线为准，须在 progress 注明选型）；ci-local.sh 同步；本地跑三命令验证 pass/fail 语义正确。

### rem-driver-contracts
- `packages/drivers/mongodb/src/mongodb.rs`、`packages/drivers/elasticsearch/src/elasticsearch.rs`、`packages/drivers/hbase/src/hbase.rs`（`command_definitions()` 含 schema catalog 命令但 `execute_command()` 不分发）。
- **验收**：二选一收敛（删定义 / 补分发）后定义集 == 处理集；加契约单测断言每个 definition 都有 execute 分支；`cargo test -p datazen-driver-{mongodb,elasticsearch,hbase}` 全绿；Host 零改动。

### rem-host-decouple
- `src/lib/mongodbFind.ts`（136 行，移入 `packages/drivers/mongodb/ui/` 并更新引用）；`ContentView.tsx:128,626`、`ConnectionPage.tsx:547-567`、`PanelContentRenderer.tsx:73`、`contentViewHelpers.ts:123,157,175`（`'redis-db'`→`meta.isKeyValue`）；`redis_flush_gate.rs:16`；`databaseTypes.ts:24-51`。
- **验收**：Host 无 `pluginId === 'redis'`/`'redis-db'` 字面分支（`types/index.ts` 注释性提及除外）；mongodbFind 引用更新后 `tsc`+vitest 全绿；面板行为不变。

### rem-sync-taxonomy（Wave2）
- `packages/driver-api/src/traits.rs`（新增 `sync_category()`/`sync_family()`，默认实现保行为）；`src-tauri/src/transfer/pairing.rs:34-61`；`src/lib/syncPairing.ts:26-77`（前端删重复分支，只读 meta）；`databaseMeta.ts`。
- **验收**：前后端重复分支删除；所有驱动行为与之前一致（单测快照/映射表）；Host 新增驱动无需改 pairing。

### rem-ddl-atomicity（Wave2）
- `src-tauri/src/services/transaction.rs:17-23` 改 trait 方法（默认 Unknown 保行为），PG/SQLite/MySQL 驱动覆写。
- **验收**：行为一致；新驱动默认 Unknown 有文档说明。

### rem-frontend-split（Wave2）
- 拆分 `tableDataStore.ts`（1302→分页/编辑缓冲/pending 三模块）、`schemaStore.ts`（789）、`panelStore.ts`（630）、`ConnectionPage.tsx`（998，抽 hooks）、`DataSyncWindow.tsx`（1374，抽 wizard hooks）；`VirtualBody` memo+行 handler `useCallback`；`WorkflowPage` type guard 替代 ~30 断言；`WorkflowChatPanel:127` zod/手写运行时校验；`extensionBridge:522,544` 注释；`schemaStore:764-767` dead branch；console.log 清理（DEV guard）。
- **验收**：对外 selector API 兼容（re-export）；`tsc`+vitest 全绿；大表滚动无功能回归（登记 E2E 留待 R）。

### rem-followups（Wave2）
- `resolve-drivers.mjs` 单测（`scripts/__tests__/resolve-drivers.test.mjs`：preset/expander/comma 解析 + registry/快照）；版本一致性脚本（package.json/tauri.conf.json/Cargo.toml）+ CI 接入；`WorkflowError` 枚举化（executor.rs `Result<_,String>` 收敛，IPC 兼容）；`save_settings` 原子化；MCP `Command::new` allowlist + 不继承 env（client.rs:206）；插件 `command:invoke` denylist/分层（extensionBridge）；CSP 收紧（tauri.conf.json:38，先评估前端直连需求）；Windows `.key`/`mcp.token` ACL；MCP allowlist 默认 deny-all（server.rs）；`testing.md` 补契约层、`e2e-coverage.md` 路径修正；L1/L2/L4/L9（Zeroize/SQLCipher评估/导入限大小/限流，按性价比取舍，progress 注明取舍）。
- **验收**：逐项在 progress 登记完成/取舍；新增脚本与单测进 CI；无行为回归。

---

## 3. 进度与 Bug 文件布局

```text
docs/development/coordination/
├── hub.md                                        # 只读生成物，禁手动改
├── deep-review-remediation-plan.md               # 本计划
└── tracks/
    ├── rem-sql-guard/{progress.md,bugs.md}
    ├── rem-key-import/{progress.md,bugs.md}
    ├── rem-ipc-redact/{progress.md,bugs.md}
    ├── rem-scheduler/{progress.md,bugs.md}
    ├── rem-panic-locks/{progress.md,bugs.md}
    ├── rem-ci-guards/{progress.md,bugs.md}
    ├── rem-driver-contracts/{progress.md,bugs.md}
    ├── rem-host-decouple/{progress.md,bugs.md}
    ├── rem-sync-taxonomy/{progress.md,bugs.md}
    ├── rem-ddl-atomicity/{progress.md,bugs.md}
    ├── rem-frontend-split/{progress.md,bugs.md}
    └── rem-followups/{progress.md,bugs.md}
```

- Bug ID：`<track-id>-BUG-nnn`；Phase 状态机见 Playbook。
- 旧 `prh-*` 轨为上一轮 hardening 遗留（均 PASSED），本轮不动。

---

## 4. 编码 / 测试完成标准（各轨通用）

```bash
node scripts/generate-builtin-locales.mjs
cargo test -p datazen --lib                # Rust 轨（驱动轨加测对应 -p datazen-driver-<id>）
npx tsc --noEmit                            # 改 TS 轨
npx vitest run <paths>                      # 改 TS 轨
```

- 禁 `pnpm install`；禁 bash 全仓 grep（用 Grep/rg 避 node_modules）；禁 add codegen / 未跟踪规格文档；`CARGO_TARGET_DIR` 独立。
- Coder 返回 `READY_FOR_TEST` + commit；Tester 全新实例四阶段后 `TEST_DONE`/`TEST_FAILED`；修复循环最多 5 轮（超限 ESCALATED）。
- 合入前提：Tester `TEST_DONE` 且 bugs 全关。合并后健全性检查同 Wave 1 策略。

---

## 5. R 阶段清单

- [ ] `cargo test -p datazen-driver-api --lib`、`cargo test -p datazen-ai-api --lib`
- [ ] `cargo test -p datazen --lib`（basic）
- [ ] basic path drivers lib 测试
- [ ] `pnpm typecheck` + `npx vitest run`
- [ ] 各轨 progress 登记的【留待 R 回归】E2E 执行
- [ ] 集成分支 → main PR（含 CHANGELOG 若有用户可见行为变化）

---

## 6. 修订记录

| 日期 | 变更 |
|------|------|
| 2026-09-03 | 初稿：深审 P0–P2 全量映射 12 轨 |
