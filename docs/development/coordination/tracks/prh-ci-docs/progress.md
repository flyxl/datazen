# Track `prh-ci-docs` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-ci-docs** 章节（CI 矩阵 / 窗口边界 / onboarding）。

## 状态

- Phase: **PASSED**
- 编码 commit: `7a6e4da0f`
- 测试 commit: `612aaf5cb`

## 设计决策

1. **CI 矩阵**：新建 `docs/development/ci-test-matrix.md`，与 `ci.yml` / `release.yml` 逐步对齐；明确 Basic 必测、All 不进 PR CI、可选 path 驱动由作者本地 + Release All SKU 覆盖、契约矩阵在 CI 外。
2. **窗口/Store 边界**：在 `docs/architecture/windows.md` §6 补充主工作区 vs 子窗口、Store 分区与 `crossWindowBus` 约定；交叉引用 `naming.md` / `state.md`。
3. **Onboarding**：`README.md` / `CONTRIBUTING.md`  toolchain 表与 CI 一致（Node 24 / pnpm 11 / Rust stable）；PR 基线命令与 CI 对齐；新增窗口/store 边界段落。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib --no-run` (basic features) | pass | 文档轨；`resolve-drivers --drivers=basic` 后编译通过 |
| n/a（文档轨） | — | 未改 TS |

## 测试代理复验（独立）

| 检查项 | 结果 | 备注 |
|--------|------|------|
| `docs/development/ci-test-matrix.md` 存在 | pass | 118 行；引用 ci.yml / release.yml |
| CI 矩阵 vs `.github/workflows/ci.yml` | pass | basic 四驱动、TS/Rust 步骤、All 不进 PR CI、Release 三 SKU 与 matrix 一致 |
| `docs/architecture/windows.md` §6 窗口/Store 边界 | pass | 主工作区 vs 子窗口、Store 分区、`crossWindowBus`、交叉引用 naming/state |
| `README.md` / `CONTRIBUTING.md` toolchain | pass | Node 24 / pnpm 11 / Rust stable；PR 基线与 ci-test-matrix §4 一致 |
| `cargo test -p datazen --lib --no-run --features driver-postgres,driver-mysql,driver-sqlite,driver-redis` | pass | Finished in 2m 46s；7 warnings（预存，非本轨引入） |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
