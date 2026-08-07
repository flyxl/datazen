# 代码审查修复进度（一期～四期+）

> 对照计划：[`code-review-2026-08-07-full.md`](./code-review-2026-08-07-full.md) · 四期+：[`2026-08-07-code-review-phase4-plus.md`](./superpowers/plans/2026-08-07-code-review-phase4-plus.md)  
> 分支：`fix/code-review-phase-1-3`  
> 流程：开发（含单元测试）→ **独立测试 Agent** 出 E2E/结果 → 失败则编码 Agent 修复 → 通过后提交。  
> **四期+ 已开始**（2026-08-07）。

## 总表

| ID | 标题 | 开发 | 单元测试 | 测试 Agent | 提交 |
|----|------|------|----------|------------|------|
| S1 | ZIP 导出排除 `.key` | ✅ | ✅ 18 lib | ✅ ADB-008 + unit | ✅ |
| S2 | 导入原子化 / 可回滚 | ✅ | ✅ 19 lib | ✅ unit+E2E | ✅ |
| S3 | SSH known_hosts / TOFU | ✅ | ✅ 11 ssh_tunnel | ✅ unit | ✅ |
| S4 | SSH 密码加密存储 | ✅ | ✅ store:: 5 | ✅ unit | ✅ |
| S5 | 路径 IPC 收紧 | ✅ | ✅ | ✅ path-ipc E2E | ✅ |
| S6 | ZIP 炸弹防护 | ✅ | ✅ 24 lib | ✅ unit+E2E | ✅ |
| S7 | MCP query 默认/硬顶 | ✅ | ✅ resolve_query_limit | ✅ unit | ✅ |
| C1 | MCP/GUI data dir 统一 | ✅ | ✅ store:: 2 | ✅ | ✅ c59281e |
| C2 | 语系真翻译或 beta | ✅ | ✅ locales 13 | ✅ | ✅ 39ef4e7 |
| C3 | Pro 文案与插件矩阵对齐 | ✅ | ⬜ N/A | ✅ 静态 | ✅ ced00bf |
| C4 | 插件钉 commit/tag | ✅ | ✅ node --check | ✅ | ✅ 13e8514 |
| C5 | 成功/错误对话框分流 | ✅ | ✅ tsc | ✅ | ✅ f28a11f |
| C6 | connection_id 语义统一 | ✅ | ⬜ N/A | ✅ 文档 | ✅ 58ab9db |
| C7 | rebuild_menu 去 block_on | ✅ | ✅ cargo check | ✅ | ✅ 14c95c5 |
| E1 | PR CI（vitest + cargo test） | ✅ | ✅ smoke | ✅ | ✅ 732e0e8 |
| E2 | `.gitignore` 粘连拆分 | ✅ | ⬜ N/A | ✅ | ✅ cadef68 |
| E3 | AGENTS / 插件默认对齐 | ✅ | ⬜ N/A | ✅ | ✅ 244b385 |
| E4 | E2E 快速构建路径 | ✅ | ✅ node --check | ✅ | ✅ 0951254 |
| E5 | Store 原子写 | ✅ | ✅ store:: 9 | ✅ | ✅ a0ee2b7 |
| E6 | DB 能力显式 opt-in | ✅ | ✅ databaseTypes | ✅ | ✅ 8129e59 |
| E7 | 死代码清理 | ✅ | ✅ tsc | ✅ | ✅ 1c19005 |
| E8 | 文档刷新 | ✅ | ⬜ N/A | ✅ | ✅ 4cb3328 |
| P4 | SQL/NL 日志降级 debug | ✅ | ✅ log_hygiene | ⬜ | ✅ ddd7ed0 |
| P5 | splash 等 bootstrap + 错误 i18n | ✅ | ✅ splash 1 | ⬜ | ✅ ba83d87 |
| C6R | connection_id → config_id 硬切换 | ✅ | ✅ mcp::server | ⬜ | ✅ 9635f73 |
| S1+ | 导出后提示并另存 `.key` | ⬜ | ⬜ | ⬜ | ⬜ |
| ConnShare | 菜单导出/导入连接 + 口令 | ⬜ | ⬜ | ⬜ | ⬜ |
| P3 | Keychain 主密钥 + 测试 fallback | ⬜ | ⬜ | ⬜ | ⬜ |
| C2F | 10 语系全量真翻译 | ⬜ | ⬜ | ⬜ | ⬜ |

图例：⬜ 未开始 · 🟡 进行中 · ✅ 完成 · ❌ 失败待修

\* E6：`databaseTypes` 套件在 `DATAZEN_PLUGINS=none` 时 kiwi 用例跳过/失败（环境性，非回归）。

**额外提交：** `2016fbc` — `refactor: move workflows out of mcp module`

## 详细记录

### S1 — ZIP 导出排除 `.key`

- **实现**：`should_exclude` 排除 `.key`；导入时若 ZIP 无 `.key` 则保留本机密钥
- **单元测试**：`cargo test -p datazen --lib app_data_archive` — 18 passed
- **提交**：本系列早期提交

### S5 — 路径 IPC 收紧

- **实现**：`*_with_dialog` 原子命令；路径 IPC 门控 `webdriver`
- **测试 Agent**：`path-ipc-hardening` 6/6、`app-data-backup` 8/8
- **提交**：本系列早期提交

### S2 / S6 / S7 — 导入原子化、ZIP 炸弹、MCP query 限流

- 见 [`code-review-2026-08-07-full.md`](./code-review-2026-08-07-full.md) 与 git log `fix/code-review-phase-1-3`

### C1 — MCP/GUI data dir 统一

- **提交**：`c59281e`

### C2 — 语系 beta 标记

- **实现**：Settings 中 de/es/fr/ja/ko/pt-BR/ru 标 `(Beta)`；`BETA_LOCALES` / `FULLY_TRANSLATED_LOCALES`；vitest 对 en key parity + English 占位率
- **单元测试**：`npx vitest run src/locales/locales.test.ts` — 13 passed
- **提交**：`39ef4e7`

### C3 — Pro 文案与插件矩阵

- **实现**：Release body + README 写明 Pro = kiwi + superset（不含 olap）；CI job 内 sequential base/pro build
- **提交**：`ced00bf`

### C4 — 插件钉 ref

- **实现**：`plugins-registry.json` 三插件 SHA；`resolve-plugins.mjs` checkout
- **冒烟**：`node --check scripts/resolve-plugins.mjs`
- **提交**：`13e8514`

### C5 — 成功/错误对话框

- **提交**：`f28a11f`

### C6 — connection_id 语义

- **实现**：`docs/architecture/backend/services.md` ID 表 + MCP 交叉链接（未做全库 rename）
- **提交**：`58ab9db`

### C6R — config_id 硬切换

- **实现**：MCP tool/prompt structs、`commands/ai.rs` db tool schemas/arg readers、`db_tools` 文档；legacy `connection_id` JSON 字段拒绝（负测）
- **单元测试**：`cargo test -p datazen --lib mcp::server::tests` — 11 passed
- **提交**：`9635f73`

### C7 — rebuild_menu async

- **实现**：`async fn rebuild_menu`，去掉 `block_on`
- **冒烟**：`cargo check -p datazen`
- **提交**：`14c95c5`

### E1–E4 — CI / gitignore / AGENTS 插件默认 / E2E minimal

- **提交**：`732e0e8`、`cadef68`、`244b385`、`0951254`

### E5 — Store 原子写

- **实现**：`write_file_atomic`；`save_json_file` / `save_encrypted_json` tmp+rename
- **单元测试**：`cargo test -p datazen --lib store::tests` — 9 passed
- **提交**：`a0ee2b7`

### E6 — supportsExplain opt-in

- **实现**：`QueryPanel` 仅 `supportsExplain === true`；内置 SQL 驱动显式 `true`
- **单元测试**：`npx vitest run src/lib/__tests__/databaseTypes.test.ts`（kiwi 需全插件构建）
- **提交**：`8129e59`

### E7 — 死代码

- **实现**：移除 `openQueryWindow`、`query` WindowKind、前端 `getSystemUiLanguage`；保留后端 IPC 供 E2E
- **冒烟**：`pnpm exec tsc --noEmit`
- **提交**：`1c19005`

### E8 — 文档刷新

- **实现**：AGENTS.md + `docs/architecture/testing.md` 计数/链接/PR CI
- **提交**：`4cb3328`

### Workflow 模块拆分

- **提交**：`2016fbc`

### P4 — SQL/NL 日志降级 debug

- **实现**：`execute_query` / AI 命令 / workflow query 步骤：info 仅记录长度与元数据，SQL/NL/错误正文/工具参数预览移至 `tracing::debug!`
- **单元测试**：`cargo test -p datazen --lib commands::query::log_hygiene_tests` — 1 passed（RED → GREEN TDD）
- **提交**：`ddd7ed0`

### P5 — splash 等 bootstrap + 错误 i18n

- **实现**：`hideSplash()` 提取至 `src/lib/splash.ts`；`bootstrap()` `finally` 隐藏 splash；移除顶层过早 hide；`MainWindow`/`queryStore` 硬编码错误串改 i18n（`backend.unknownError`、`query.cancelled`）
- **单元测试**：`npx vitest run src/lib/__tests__/splash.test.ts` — 1 passed；`pnpm exec tsc --noEmit` — pass
- **提交**：`ba83d87`

## 延期 / 范围外

| ID | 说明 |
|----|------|
| P1–P2 | 四期性能（LTO/CI、`max_tokens` 默认），本次不执行 |
| E6 其余标志 | `supportsErDiagram` 仍为 `!== false`，仅 EXPLAIN 改为 opt-in |
| E5 流式备份 | 仅 Store JSON/加密文件原子写；整库 ZIP 流式/`pg_dump` 未做 |
