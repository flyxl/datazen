# Brief: sql-s2-d — 风险 classifier 与 Host guard 回归

> 依赖：S2-A、S2-C 已 PASSED 并合流；消费 S2-C 拆出的 Rust 模块目录（`sql_guard/`），只改 safety 相关文件。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S2-D（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s2-d/progress.md`。

## 独占写锁

可建/可改：`src/lib/dangerousSql.ts`（或 query risk 模块）、
`src/windows/connection/query/queryExecutionRisk.ts`、
`src-tauri/src/sql_guard/safety.rs`、对应 TS/Rust tests。
**禁止**：`params.rs` / `scanner.rs`（S2-C 遗产）、`mod.rs` 导出结构（要加导出先停下找协调者仲裁）。

## 步骤

1. 实现 read / mutation / unknown 分类 + drop / truncate / no-WHERE findings。
2. WHERE 必须位于当前 DML 顶层；排除 CTE、subquery、字符串、注释（用 S2-A scanner 的词法区判定）。
3. 多语句汇总全部 findings 和最高风险，保留原文 range。
4. 保持 Rust `readOnly` / Safe Mode 行为（含注释内写动词拦截与空块注释处理），不增加确认绕过参数。
5. 验证 binder→guard 顺序由 execute / streaming 共用同一路径。

## 测试

顶层 WHERE、CTE/subquery 排除、字符串中 WHERE、多语句、unknown、
readOnly / Safe Mode、execute/stream 一致性。

## 完成定义

- 前端 assessment 与 Host guard 的共享 fixture 无语义漂移；既有 guard 测试全绿。
- 相关验收：AC-17（Safe Mode/readOnly 无绕过；production/high-risk 统一确认逻辑在本轨只做分类，确认 UI 由 S5-C 做）。
