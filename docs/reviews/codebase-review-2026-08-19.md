# DataZen 全代码库审查报告

**日期：** 2026-08-19  
**范围：** 整个 DataZen 仓库（非仅近期变更）  
**分支：** `feat/unified-connection-tree`  
**方法：** 阅读 `AGENTS.md` 与 `docs/architecture/` → 后端 / 前端 / 驱动 / 测试 / 构建 / i18n 系统性走查

---

## 执行摘要

DataZen 是基于 Tauri v2 的跨平台数据库管理工具（v0.0.9），整体架构清晰：**Driver Command API** 驱动行为差异、**Workflow/MCP** 共用 runtime、**路径 IPC 门闸** 与 **AES-256-GCM** 加密等安全基线扎实。

全库规模约：**161** 个 Rust Host 模块、**484** 个 TS/TSX 文件、**18** 个驱动（15 path + 3 git）、**60** 个 E2E spec、**165+** Vitest 文件。

主要风险集中在：**SQL 安全策略在不同 IPC 路径上不一致**、**i18n 十语系承诺与运行时两语系不同步**、**CI 仅覆盖 basic 驱动 SKU 且无 E2E**。多数发现为 P2–P3 的可维护性与策略一致性问题，而非架构性缺陷。

| 严重级别 | 数量 | 代表主题 |
|----------|------|----------|
| P0 | 0 | — |
| P1 | 3 | SQL 管道绕过 sql_guard；只读连接 UI 未门闸；十语系承诺未落地 |
| P2 | 18 | CI 缺口、死代码、模块重复、i18n 管线未接入构建 |
| P3 | 17 | 命名、驱动拆分、文档漂移、缓存调优 |

---

## 正面观察

| 领域 | 评价 |
|------|------|
| **架构分层** | Commands → Services → Driver Command Runtime → Drivers，与 `docs/architecture/README.md` 一致 |
| **可扩展性** | 前端零 `pluginId === 'redis'` 硬编码；Redis 深度能力在驱动 crate 内 |
| **构建选型** | `resolve-drivers.mjs` 编译期 SKU + stash/restore；CI guard 防止误提交 codegen |
| **核心安全** | `sql_guard.rs` 20+ 单测；对话框路径文件 IO；日志脱敏 `log_redact.rs` |
| **Data Sync** | `data_sync/gate.rs` 同族/PK/基表门闸设计明确且有测试 |
| **性能** | 查询流式 batch、restore 80ms 进度节流、Schema 双层 TTL 缓存 |
| **测试文化** | Host 单测覆盖广；E2E 契约矩阵（`e2e/contract/`）是良好模式 |
| **状态迁移** | history/favorites 迁到 SQLite；panelStore 合并 queryExec |

---

## 一、架构

### [A-01] P2 — `sync/` 与 `data_sync/` 命名易混淆

- **路径：** `src-tauri/src/sync/` vs `src-tauri/src/data_sync/`
- **问题：** 前者为 Transfer/IR 适配，后者为 V1 Diff Sync 引擎；文档有说明但代码导航成本高
- **建议：** 将 `sync/` 重命名为 `transfer/`，或在各模块顶栏加 `//!` 说明

### [A-02] P2 — 11 个驱动注册 `sync_adapter` 但 V1 Sync 仅支持 mysql/postgresql 族

- **路径：** `packages/drivers/*/sync_adapter.rs`，`data_sync/pairing.rs`
- **建议：** 用 feature 门闸 adapter 注册，或明确标注为 Transfer 预览

### [A-03] P3 — RFC unified-panel-store 部分落地

- **路径：** `docs/architecture/rfc/unified-panel-store.md`，`panelStore.ts`
- **问题：** queryExec 已合并，但 `SqlFilePanel` 类型仍存在（已无创建路径）
- **建议：** 更新 RFC 状态；清理死代码

### [A-04] P3 — 驱动 wire reuse 不透明

- **路径：** `drivers-registry.json`，`packages/driver-api/src/reuse.rs`
- **建议：** 在 `docs/architecture/backend/drivers.md` 增加 reuse 映射表

---

## 二、安全

### [S-01] P1 — Restore / 执行 SQL 文件绕过 `sql_guard`

- **路径：** `src-tauri/src/commands/backup.rs`，`sql_guard.rs`
- **问题：** 查询路径检查 `read_only` 与 Safe Mode；restore / SQL 文件流式执行**无同等检查**
- **建议：** 在 `restore_database_from_path` 入口拒绝只读连接；可选对 Safe Mode 做动词拦截（参考 `commands/sync/exec.rs`）

### [S-02] P1 — 前端「执行 SQL 文件」未对只读连接门闸

- **路径：** `src/lib/schemaTreeContextMenu.ts`，`ExecuteSqlFileDialog.tsx`
- **问题：** Import/Drop 尊重 `readOnly`；`execute-sql-file` 无；`ContentView` 的 `ctxIsReadOnly` 来自驱动元数据而非 `ConnectionConfig.readOnly`
- **建议：** 菜单与对话框双层拦截，与 S-01 配套

### [S-03] P2 — `apply_params` 将 JSON 内联为 SQL 字面量

- **路径：** `commands/driver_command.rs`
- **建议：** 优先 `driver.query_with_params()` 传参，内联仅用于预览

### [S-04] P2 — CSP 允许明文 `http:`

- **路径：** `src-tauri/tauri.conf.json`
- **建议：** 尽量收窄 connect-src；文档记录各驱动例外

### [S-05] P2 — MCP 连接 allowlist 默认为「允许全部」

- **路径：** `mcp/allowlist.rs`
- **建议：** 考虑默认 deny + 显式 allowlist；设置页提示

### [S-06] P2 — SQL 安全策略矩阵缺失

- **路径：** query / workflow / data_sync / backup 各路径
- **建议：** 文档化「哪些 IPC 走 sql_guard」，并逐步统一到单一 pre-execution hook

### [S-07] P3 — `style-src 'unsafe-inline'`

- **说明：** Tailwind 常见权衡；跟踪 nonce 方案

---

## 三、测试

### [T-01] P2 — CI 不跑 E2E

- **路径：** `.github/workflows/ci.yml`
- **建议：** nightly 或可选 PR job 跑 `pnpm e2e:minimal`

### [T-02] P2 — CI 仅测 4/15 path 驱动

- **路径：** ci.yml，`drivers-registry.json`
- **建议：** main/nightly 增加 `DATAZEN_DRIVERS=all` 的 `cargo test`

### [T-03] P2 — 3 个 git 驱动无 CI

- **路径：** Kiwi / OLAP / Superset，仅在 release.yml 克隆
- **建议：** 定期对 pinned ref 做集成

### [T-04] P3 — ER 图 E2E 为 Partial

- **路径：** `docs/e2e-coverage.md`，`e2e/specs/er-diagram.ts`

### [T-05] P3 — 集成测试依赖外部 PG/MySQL/LLM

- **路径：** `src-tauri/tests/workflow_tests.rs`，`ai_e2e.rs`

### [T-06] P3 — Turso/RQLite 单测极少

- **路径：** `packages/drivers/turso/`，`rqlite/`

---

## 四、前端

### [F-01] P2 — `SqlFilePanel` 死代码

- **路径：** `SqlFilePanel.tsx`，`panelStore.ts`，`PanelContentRenderer.tsx`
- **说明：** 已改为 `ExecuteSqlFileDialog` + 后端流式执行

### [F-02] P2 — SQL 文件执行 / 恢复成功后未刷新 Schema

- **路径：** `ExecuteSqlFileDialog.tsx`，`BackupWindow.tsx`

### [F-03] P3 — `ConnectionWindow.tsx` 体量过大（~667 行）

- **建议：** 拆分子 hook

### [F-04] P3 — 少量 `@ts-ignore` / `eslint-disable`

- **路径：** `SqlEditor.tsx`，`WorkflowYamlEditor.tsx` 等

### [F-05] P3 — 生产代码几乎无 `any`（良好，应 CI 保持）

---

## 五、后端（Rust Host）

### [B-01] P2 — `require_webdriver_path_ipc` 重复

- **路径：** `commands/backup.rs`，`commands/config.rs`
- **建议：** 提取 `commands/path_ipc.rs`

### [B-02] P2 — SQL 关键字解析在 `sql_guard` 与 `mcp/permission` 重复

- **建议：** 共享 `sql_parse` 模块

### [B-03] P3 — `QueryExecutor::execute_query` 标记 dead_code

### [B-04] P3 — `SchemaCache` 部分字段未使用

### [B-05] P3 — 连接导入面大（Navicat/DBeaver/…），需 fuzz/大小限制审计

- **路径：** `commands/connection_import/`

---

## 六、驱动

### [D-01] P2 — 核心驱动 crate 过大

- **路径：** postgres ~2022 LOC，mysql ~1953，redis ~1688
- **建议：** 按 connect/query/structure/admin 拆模块（参考 Redis `ops_*.rs`）

### [D-02] P2 — MongoDB 仅 9 个单测

### [D-03] P3 — HTTP 驱动错误处理不统一

- **路径：** `http-support/`，各 HTTP 驱动

### [D-04] P3 — Redis 驱动架构可作为插件开发模板（正面）

---

## 七、DevEx / 构建

### [X-01] P2 — `resolve-drivers.mjs` ~1200 行，上手成本高

- **建议：** 增加 `--dry-run` 输出选型摘要

### [X-02] P2 — Release 构建 `all` SKU，CI 仅 `basic`

- **路径：** `release.yml` vs `ci.yml`

### [X-03] P3 — 首次构建需 codegen，易绊倒新贡献者

- **建议：** `tauri:dev` 启动时检测并提示

### [X-04] P3 — Guard 脚本优秀；建议 CI 接入 `i18n-sync-check.mjs`

---

## 八、性能

### [P-01] P3 — Schema cache `max_tables: 1000` 大库可能抖动

### [P-02] P3 — Workflow 查询行数硬 cap 1000

- **路径：** `workflow/executor.rs`，`mcp/server.rs`

### [P-03] P3 — 连接 idle 30 分钟，可考虑设置暴露

---

## 九、i18n

### [I-01] P1 — 「十语系」与运行时两语系不一致

- **路径：** `src/locales/index.ts`（en/zh-CN），`e2e/specs/i18n-10-locales.ts`
- **问题：** E2E 期望 10 语言；运行时仅 2；`scripts/locale-data/` 有数据但未接入 bootstrap
- **建议：** **要么** 接入 `pnpm i18n:build` 管线 **要么** 下调 E2E/文档/设置页预期

### [I-02] P2 — TS / Rust / 菜单三处 locale 列表发散

- **路径：** `resolveUiLanguage.ts`，`i18n_locale.rs`，`generate-menu-labels.mjs`

### [I-03] P2 — 翻译脚本存在但未进 build pipeline

- **路径：** `scripts/generate-locale-json.mjs`，`build-zh-tw.mjs` 等

### [I-04] P3 — `docs/architecture/testing.md` 与 `locales.test.ts` 不一致

### [I-05] P3 — AGENTS.md「开发期只改 en + zh-CN」流程合理（正面）

---

## 十、近期 SQL 文件执行改造（变更子集）

本节为近期功能变更的补充，全库视角下仅为 F-01 / S-01 等相关项的具体实例。

| 项 | 状态 |
|----|------|
| 与 restore 共用 `sql_file_with_dialog` + 流式执行 | ✅ 已完成 |
| 对话框内执行、不展示 SQL 全文 | ✅ 已完成 |
| 确认文案（破坏性语句警告，非「删除现有表/视图」） | ✅ 已修正 |
| 错误状态红色样式 | ✅ 已修正（`ExecuteSqlFileDialog` `statusKind=error`） |
| 只读 / Safe Mode 门闸 | ❌ 见 S-01、S-02 |
| 执行后 Schema 刷新 | ❌ 见 F-02 |
| Host E2E / 单测 | ❌ 见 T-01 |

---

## 建议行动顺序

1. **P1：** S-01 + S-02 只读/Safe Mode 统一；I-01 十语系决策
2. **P2：** F-01 删 SqlFilePanel；F-02 Schema 刷新；T-01/T-02 CI 扩展；B-01/B-02 模块 Consolidation
3. **P3：** 驱动拆分、文档同步、缓存与 workflow cap 可配置化

---

## 附录：仓库规模

```text
src-tauri/src/     ~161 Rust 文件
src/               ~484 TS/TSX 文件
e2e/specs/         60 spec 文件
packages/drivers/  15 path + 3 git 驱动
内置 UI 语言       2（en, zh-CN）；脚本数据 8+ 未完全接入
CI 驱动测试        postgres, mysql, sqlite, redis
```

---

## 审查方法说明

- 通读 `AGENTS.md`、`docs/architecture/README.md`、security/testing 文档
- 抽样 Host commands、services、stores、windows 主路径
- 检查驱动 registry、driver-api trait、path/redis 驱动代表实现
- 对照 CI workflow、E2E coverage 矩阵、i18n 脚本与运行时
- 运行 Host 相关单测抽样验证（backup、BackupWindow、ContentView）

*本报告覆盖全代码库；随版本演进应更新优先级与「已修复项」。*
