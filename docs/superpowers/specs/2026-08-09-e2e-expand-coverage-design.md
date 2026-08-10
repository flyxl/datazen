# E2E 扩展与双轨覆盖率（C+D）

> **Status:** Implemented (measurement complete; coverage optimization awaiting review)  
> **Branch / worktree:** `test/e2e-expand-coverage` @ `.worktrees/test-e2e-expand-coverage`  
> **Date:** 2026-08-09  
> **Plan:** `docs/superpowers/plans/2026-08-09-e2e-expand-coverage.md`  
> **Matrix:** `docs/superpowers/specs/2026-08-09-e2e-tc-coverage-matrix.md`  
> **Optimization proposal:** `docs/superpowers/specs/2026-08-09-coverage-optimization-proposal.md`

## 1. Context

DataZen 已有约 40 个 WebdriverIO E2E spec，以及手工黑盒用例集 `test/test-cases.md`（约 100+ `TC-*`）。前端 Vitest 覆盖率配置目前仅 include 两个语言相关文件，阈值 75%；Rust 可用 `cargo llvm-cov`，但仓库未对「全库 80%」做强制门槛。

本工作目标：

1. 在隔离 worktree 新分支上**尽可能多地**补齐可自动化 E2E。
2. 完整跑测，并按 **C+D** 统计覆盖率：
   - **C**：前端 Vitest 行覆盖率 + Rust `llvm-cov` 行覆盖率，分别对照 **80%**。
   - **D**：`test/test-cases.md` 场景覆盖率（TC → E2E 映射）。
3. 任一侧代码行覆盖率 <80% 时，**只提出优化方案供审核**，本轮不擅自大批量补单测或改产品逻辑。

## 2. Goals

1. 为本机可连服务写好 `e2e/.env`（gitignored），并自建 SQLite fixture。
2. 修正现有 E2E 中硬编码 `postgres` 用户导致本机失败的问题：统一读 `E2E_PG_*`。
3. 新增/扩展 E2E，优先覆盖尚未自动化的 **P0/P1**（及稳定 P2）黑盒用例。
4. 产出 TC↔E2E 覆盖矩阵文档（D）。
5. 扩大 Vitest coverage `include` 到有意义的前端源码范围；跑 `cargo llvm-cov -p datazen --lib`；报告数字。
6. 若 C 任一侧 <80%，交付书面优化方案，等待审核后再执行。

## 3. Non-goals

- 不为冲 80% 而缩小 coverage `include` 或排除难测大文件造假。
- 本轮不修改 CI 强制阈值（除非后续审核通过的优化方案明确要求）。
- 不自动化依赖真实 SSH 隧道、外部 MCP Server、Redis Cluster/Sentinel、无 API Key 的 AI/Kiwi 的强制通过用例（保留 skip + 矩阵标注）。
- 不把手工黑盒 `computer-use-mcp` 流程改成默认 CI 门禁。
- 不为覆盖率而重构无关产品代码。

## 4. Local environment (approved)

| Service | Endpoint | Credentials for this machine |
|---------|----------|------------------------------|
| PostgreSQL | `127.0.0.1:5432` | user=`wuxiaolong`, password=（空）, db=`postgres` |
| MySQL | `127.0.0.1:3306` | user=`root`, password=（空）, db=`datazen_test` |
| Redis | `127.0.0.1:6379` | 无密码 |
| SQLite | file fixture | `node e2e/create-sqlite-test-db.mjs` → `e2e/fixtures/test.db` |

`e2e/.env` 仅存在于本地 worktree，**不提交**。`.env.example` 可更新注释说明 Homebrew 本机用户常见情况。

## 5. Approach (approved: hybrid C)

| Phase | Work |
|-------|------|
| A | 环境：`pnpm install`、写 `.env`、生成 SQLite、修正 PG 用户读取 |
| B | E2E：按下方清单新增/扩展 spec |
| C-measure | 扩 Vitest coverage include；跑 unit coverage + llvm-cov + 全量/分组 E2E |
| D-report | TC 矩阵 + 行覆盖率报告；<80% 则写优化方案待审 |

## 6. E2E additions (priority)

### 6.1 Must-add / expand

| Spec (new or expand) | TC IDs | Notes |
|----------------------|--------|-------|
| `connection-validation.ts` (new) | CONN-005/006/007, EDGE-007 | 必填、无效 Host、错误密码、空密码；以 UI + 测试连接为主 |
| `hotkeys.ts` (new) | HOTKEY-001~005 | Cmd+N / `,` / Enter / W / B；WebKit 下用 `browser.keys` 或派发键盘事件 |
| Expand `sql-query.ts` | QUERY-006/008 | 取消运行中查询、查询历史面板（收藏已有 SQ-015+）；不另开文件 |
| `edge-cases.ts` (new) | EDGE-001/002/004/008 | 超长连接名、特殊字符库名（SQLite/PG 可建）、大结果集、快速重复操作 |
| `chart-views.ts` (new) + keep `chart-expand.ts` | CHART-002~008/012 | 类型切换、轴、推荐、空状态、表图联动；放大仍由 `chart-expand.ts` 负责 |
| Expand `settings.ts` | SET-003/004/007 | 字体、数据浏览、Prompt 入口可见性（不连外部 LLM） |
| `ui-window-ops.ts` (new) | UI-001/002/003/005 | 多连接窗口、重复打开、侧栏尺寸、状态栏 |
| Expand `table-data.ts` / related | TABLE-004/008/009 | 筛选、多选、空表 |
| Env / helpers fix | — | `helpers.ts`、`wdio.conf.ts`、硬编码 `postgres` 的 spec 统一 `E2E_PG_USER` |

### 6.2 Conditional (skip if env missing)

- AI / Kiwi：无 `E2E_AI_*` / `E2E_KIWI_*` → skip（现有行为保留）
- Redis topology Cluster/Sentinel：未配置 → skip
- SSH、外部 MCP、SYNC-004：矩阵标「无法稳定自动化」

### 6.3 Implementation constraints

- 遵循现有 `e2e/helpers.ts` / `invokeBackend` / `data-*` 选择器模式。
- 新用例 ID 在 describe/it 中标注对应 `TC-*`，便于 D 矩阵扫描。
- 禁止 mock 冻结的 `__TAURI_INTERNALS__.invoke`（见 `docs/e2e-testing.md`）。
- E2E 二进制必须经 `pnpm e2e` / Tauri CLI + `webdriver`；禁止裸 `cargo build`。

## 7. Coverage measurement (C)

### 7.1 Frontend (Vitest)

- 命令：`pnpm test:unit:coverage`
- 将 `vitest.config.ts` → `coverage.include` 扩展为应用源码（建议）：
  - include：`src/**/*.{ts,tsx}`（排除 `src/plugins/generated.ts`、纯类型声明若无语句）
  - exclude：`**/*.test.*`、`**/__tests__/**`、`src/locales/**`（大体量翻译表，场景由 i18n E2E/单元 key parity 覆盖）、`src/plugins/generated.ts`
- 阈值：本轮**报告**对照 80%；**不**立刻把 CI threshold 提到 80%（避免假失败）。现有 75% 阈值若因 include 扩大而失败，可临时改为仅 report（或提高 include 后先关闭 thresholds，在优化方案中再设门禁）。

### 7.2 Rust (`cargo llvm-cov`)

- 命令：`cargo llvm-cov -p datazen --lib --summary-only`（或 HTML 报告到 `target/llvm-cov-e2e-expand`）
- 对照 **lines 80%**；不足则列入优化方案（优先测 `commands/`、`services/`、`store/`、`workflow/` 等纯逻辑）。

### 7.3 What “full test run” means this round

1. `pnpm test:unit:coverage`
2. `cargo test -p datazen --lib`（基线）+ `cargo llvm-cov -p datazen --lib`
3. `pnpm e2e`（本机 PG/MySQL/Redis/SQLite；AI/Kiwi/topology 按 env skip）

## 8. Scenario coverage (D)

Deliverable：`docs/superpowers/specs/2026-08-09-e2e-tc-coverage-matrix.md`

每行一个 `TC-*`，列：

| 字段 | 含义 |
|------|------|
| Status | `covered` / `partial` / `skip` / `gap` / `manual-only` |
| Spec | `e2e/specs/...` 或空 |
| Notes | 环境依赖、为何无法自动化 |

场景覆盖率 = `(covered + partial×0.5) / total TC`（公式写在矩阵文档首部）。本轮目标是**尽量提高**，不设硬性 80% 场景门槛（80% 门槛仅适用于 C 的行覆盖率；场景侧如实报告）。

## 9. Optimization proposal gate

若 Vitest 或 llvm-cov **任一侧 lines < 80%**：

1. 停止「为冲覆盖率而写产品代码」。
2. 输出优化方案文档（可附在矩阵同目录或 progress 笔记），内容包括：高缺口目录、建议单测清单、预估收益、风险、是否值得把 E2E 换成单测。
3. **等待用户审核**后再执行。

## 10. Risks

| Risk | Mitigation |
|------|------------|
| Homebrew PG 非 `postgres` 角色 | `.env` + 统一 env 读取 |
| 全量 E2E 耗时长 / 偶发 flaky | 先分组验证新 spec，再全量；失败如实报告 |
| Vitest include 扩大后覆盖率骤降 | 预期内；走第 9 节方案门 |
| 热键在 WebDriver/WebKit 不稳定 | 失败则降级为「打开对应 UI 入口」断言 + 矩阵标 partial |
| 无 `e2e/.env` 误提交 | 确认 `.gitignore` 已忽略；只更新 `.env.example` |

## 11. Success criteria

- [ ] worktree 分支上新增/扩展的 E2E 已合并进 `e2e/specs/`
- [ ] 本机 PG/MySQL/Redis/SQLite 相关用例可跑（或明确失败原因）
- [ ] TC 矩阵文档已生成
- [ ] Vitest 与 llvm-cov 数字已记录
- [ ] 若 <80%，优化方案已提交用户审核（未擅自执行）

## 12. Out-of-scope follow-ups (after approval of optimization plan)

- 按方案补前端/Rust 单测冲 80%
- CI threshold 调整
- 场景覆盖硬门槛
