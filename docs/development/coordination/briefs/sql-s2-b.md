# Brief: sql-s2-b — 轻量 scope model、relation resolver 与 dialect core

> 依赖：S2-A 已 PASSED 并合流（base commit 以派发时协调者给的为准）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S2-B（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s2-b/progress.md`。

## 独占写锁

可建/可改：`src/components/sql-editor/semantic/scopeModel.ts`、`relationResolver.ts`、
`dialectAdapter.ts`、通用 quote helper、类型文件、本轨纯单测。
**禁止**：`scanner.ts` / `statementRanges.ts` / `tokens.ts`（S2-A 基线遗产，只读消费）、
`sqlCompletions.ts` / `sqlCompletionContext.ts`（冻结，实际迁移由 S4-B 做）。

## 步骤

1. 从 S2-A token stream 识别 SELECT/INSERT/UPDATE/DELETE 的 scope 边界。
2. 提取 FROM/JOIN relation、database/schema qualification、显式/隐式 alias。
3. 解析 WITH CTE、显式 CTE 列和可确定 projection。
4. 建立 parent scope 与 alias shadowing。
5. 识别 cursor intent：relation、qualified column、projection、join target、function call、INSERT values。
6. relation resolver 输出唯一 / 歧义 / 未解析三态，不直接访问 store。
7. 对不完整 SQL 返回部分结果，不因孤立括号/关键字 throw。

## 测试（纯输入/输出，无 React/store/IPC）

alias、quoted identifier、CTE、nested query、shadowing、同名表、多 schema、
incomplete SQL、自关联。

## 完成定义

- 所有测试仅用纯输入/输出；没有 React、store、IPC 依赖。
- 单生产文件 <500 行。
- 相关验收：AC-04（alias/CTE/scope 容错，歧义不猜）、AC-21（无 dbx 痕迹）。
