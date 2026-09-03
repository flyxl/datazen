# DataZen 代码 Review 后硬化 PRD（Post-Review Hardening）

> **状态**：Draft  
> **优先级**：P0 / P1（见各任务）  
> **目标版本**：v0.1.x → v0.2.0 前持续落地  
> **来源**：对 [flyxl/datazen](https://github.com/flyxl/datazen) 的架构与代码静态 Review  
> **影响范围**：`src-tauri/`、`packages/driver-api`、`src/`、CI、文档与外部契约（MCP / 插件 / 导出格式）  
> **实施计划**：`docs/development/coordination/post-review-hardening-plan.md`

## 1. 文档目的

本文把一次全仓 Code Review 中识别出的**待优化项**收敛为一份可执行 PRD：明确问题、目标、非目标、任务拆分、验收标准与优先级。

本 PRD 不扩大产品功能面，聚焦工程质量、安全边界与可演进性。

## 2. 背景与问题清单

| ID | 问题 | 风险 | 优先级 |
|----|------|------|--------|
| H1 | 超大源文件（`mcp/server.rs`、`commands/driver_command.rs`、`lib.rs` 等 1k–1.7k 行） | Review 成本高、回归面大 | 高 |
| H2 | 生产路径较多 `unwrap` / `expect` | 非测试路径 panic | 高 |
| H3 | `sql_guard` 为启发式防护 | 用户预期过高；边界可绕过 | 高 |
| H4 | AI 上下文默认可能附带高敏感数据出域 | 第三方 LLM 泄露 | 高 |
| H5 | 外部契约破坏性改名无 deprecation 窗口 | 插件/MCP 生态迁移成本 | 中 |
| H6 | 驱动 SKU 组合多，CI 全量成本上升 | 覆盖不足或流水线过慢 | 中 |
| H7 | 前端窗口与 Zustand store 面大 | 状态漂移 | 中 |
| H8 | 安全路径与 MCP 契约专项回归不足 | 静默漂移 | 中 |
| H9 | 贡献者 toolchain 说明分散 | Onboarding 成本 | 低 |

## 3. 目标与非目标

**目标**：可维护性（拆大文件）、稳定性（生产路径错误处理）、安全预期可管理（sql_guard / Safe Mode）、AI 出域可控、契约可演进、关键路径可回归。

**非目标**：不合并 Sync/Diff/Transfer 产品语义；不实现形式化 SQL 证明；不引入企业审计/多租户/Web 平台；不强制清零测试中的 `unwrap`。

## 4. 任务索引（与实施计划 Track 映射）

| PRD 任务 | Track ID | 优先级 |
|----------|----------|--------|
| T1 拆分超大后端模块 | `prh-split-mcp` / `prh-split-dcmd` / `prh-split-lib` | P0 |
| T2 生产路径 panic 治理 | `prh-panic-policy` | P0 |
| T3 SQL 安全边界 | `prh-sql-guard` | P0 |
| T4 AI 出域默认策略 | `prh-ai-egress` | P0 |
| T5 外部契约 Deprecation | `prh-contract` | P1 |
| T6 CI 矩阵策略 | `prh-ci-docs` | P1 |
| T7 前端窗口/Store 边界 | `prh-ci-docs`（文档轨合并） | P1 |
| T8 安全与契约专项测试 | 并入 `prh-sql-guard` / `prh-ai-egress` / `prh-contract` | P1 |
| T9 Onboarding | `prh-ci-docs` | P2 |

详细验收与落点见实施计划。

## 5. 修订记录

| 日期 | 变更 |
|------|------|
| 2026-09-03 | 初稿 |
| 2026-09-03 | 增加实施计划链接与 Track 映射 |
| 2026-09-03 | 与实施计划合并至单一分支 `feat/post-review-hardening` |
