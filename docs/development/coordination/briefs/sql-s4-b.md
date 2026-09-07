# Brief: sql-s4-b — 补全、FK JOIN 与函数签名

> 依赖：S2-B（scope/relation）、S3-A（metadata snapshot）。
> 只交付 factory + 纯逻辑模块，**不修改** metadata loader / editor composer。
> 冻结接口：本轨是 `sqlCompletions.ts` / `sqlCompletionContext.ts` 兼容 wrapper 的**唯一** owner。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S4-B + §6.2（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s4-b/progress.md`。

## 独占写锁

可建/可改：`src/components/sql-editor/completion/`、function registry、signature extension、
`src/lib/sqlCompletions.ts`、`src/lib/sqlCompletionContext.ts` 及测试。
**禁止**：metadata loader、editor composer。

## 步骤

1. 保留现有基础 schema completion 作 fallback。
2. `alias.` 唯一解析时只返回该 relation 的列（detail：type/nullable/comment）；歧义时不猜。
3. relation completion 用当前 scope 的 database/schema 上下文。
4. JOIN completion 从 snapshot FK 双向构建候选；支持复合键、自关联、多关系分别列出。
5. 选中 JOIN 候选插入 quoted relation、唯一 alias、完整 ON 条件；不得覆盖用户已有 ON。
6. 函数 completion 与 signature help 共用 registry；signature 按 comma depth 高亮当前参数。
7. registry 由 dialect 元数据/通用集合组合，不写 driver ID switch。
8. 初始 registry 至少覆盖 `DATE_ADD`、`CONCAT` 及现有 common/PG/MySQL/SQLite 清单；
   旧 `sqlFunctionCompletions` 改为该 registry 的兼容投影。

## 测试

alias filter、CTE、ambiguous、类型注释、复合/多 FK、自关联、现有 ON、嵌套函数、缺元数据 fallback。

## 完成定义

- completion/source 回调同步完成且不发 IPC；函数信息单一来源。
- 相关验收：AC-05、AC-06、AC-07。
