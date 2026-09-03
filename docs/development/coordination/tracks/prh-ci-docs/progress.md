# Track `prh-ci-docs` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-ci-docs** 章节（CI 矩阵 / 窗口边界 / onboarding）。

## 状态

- Phase: **PASSED**
- 编码 commit: `7a6e4da0f`（合入 `3b89aa556`）
- 测试 commit（初轮）: `be9270e4e`
- 测试 commit（强化）: _pending commit_

## 设计决策

1. **CI 矩阵**：新建 `docs/development/ci-test-matrix.md`，与 `ci.yml` / `release.yml` 逐步对齐；明确 Basic 必测、All 不进 PR CI、可选 path 驱动由作者本地 + Release All SKU 覆盖、契约矩阵在 CI 外。
2. **窗口/Store 边界**：在 `docs/architecture/windows.md` §6 补充主工作区 vs 子窗口、Store 分区与 `crossWindowBus` 约定；交叉引用 `naming.md` / `state.md`。
3. **Onboarding**：`README.md` / `CONTRIBUTING.md` toolchain 表与 CI 一致（Node 24 / pnpm 11 / Rust stable）；PR 基线命令与 CI 对齐；新增窗口/store 边界段落。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib --no-run` (basic features) | pass | 文档轨；`resolve-drivers --drivers=basic` 后编译通过 |
| n/a（文档轨） | — | 未改 TS |

## 测试代理复验（独立）

### 阶段 A — 代码审查

| 检查项 | 结果 | 备注 |
|--------|------|------|
| `ci-test-matrix.md` vs `ci.yml` | **mostly pass** | 步骤/驱动/basic SKU/All 不进 PR CI 一致；ai-api 顺序见 BUG-004 |
| `ci-test-matrix.md` vs `release.yml` | pass | Basic / All / Akulaku 三 SKU × 四平台与 matrix 一致 |
| `windows.md` §6 vs 代码 | **partial** | Store 列表与 `src/stores/*` 一致；子窗口不读写 `panelStore` 成立；`connection-ready` 监听描述有误（BUG-002）；§4 片段缺 `data-transfer`（BUG-001） |
| README / CONTRIBUTING toolchain | pass | Node 24 / pnpm 11 / Rust stable 与 `ci.yml` 一致 |
| `crossWindowBus` 事件 | partial | `settings-changed`、`connections-changed` 与代码一致；§6.3 `connection-ready` 子窗口消费描述不准确 |

### 阶段 B — 独立复验

| 套件 | 编码代理自报 | 独立实测 | 结果 |
|------|-------------|----------|------|
| `cargo check -p datazen --lib` (basic features) | pass (--no-run) | Finished 4m 13s, 16 warnings | pass |
| `npx vitest run scripts/__tests__/check-ci-docs-consistency.test.ts` | n/a | 5 passed / 5 | pass |
| `node scripts/check-ci-docs-consistency.mjs` | n/a | exit 1（doc drift BUG-001） | expected |

### 阶段 C — 新增测试

| 测试 | 路径 | 说明 |
|------|------|------|
| CI 驱动 registry 一致性 | `scripts/check-ci-docs-consistency.mjs` + vitest | 提取 `ci-test-matrix.md` 驱动 id，校验 `drivers-registry.json` |
| 窗口边界断言 | 同上 `checkWindowBoundaries()` | 子窗口 kind / singleton label vs `windowKind.ts` + `windowManager.ts` |
| Toolchain 版本 | 同上 `checkToolchainVersions()` | README / CONTRIBUTING / ci-test-matrix vs `ci.yml` Node 24 / pnpm 11 |
| CLI 入口 | `pnpm test:ci-docs` | `package.json` script |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

- 4 项文档漂移已登记 `bugs.md`（BUG-001–004）；代码路径无回归。
