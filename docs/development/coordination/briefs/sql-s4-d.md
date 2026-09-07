# Brief: sql-s4-d — Hover 与定义导航（S4-A/B/C 合流后启动）

> 依赖：S4-A、S4-B、S4-C 已合流；S3-A（metadata snapshot）；S3-B2（navigation callbacks）。
> 只交付 factory，**不修改** composer、ContentView、QueryPanel。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S4-D + §6.5（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s4-d/progress.md`。

## 独占写锁

可建/可改：`tableHover.ts`、`definitionNavigation.ts` 及测试。
**禁止**：composer、ContentView、QueryPanel。

## 步骤

1. hover 延迟 300ms；移出/文档变更取消；展示 database/schema、PK、index、核心列；
   comment 不存在隐藏（不伪造；表注释强制显示需另立 Driver API 项目，不在本轨）。
2. 列清单和 tooltip 内容有上限，大表显示省略计数。
3. Hover actions 和 Mod/Ctrl-click 用同一个 resolver/callback；默认 click 开数据页，
   结构页由 hover 明确动作进入。
4. Copy DDL 只发既定 callback，不从 metadata 拼 DDL。
5. identity 不唯一 / view 不支持某动作 / metadata 缺失时隐藏对应动作。

## 测试

hover delay/cancel、identity、modifier click、动作可见性、复制 DDL callback。

## 完成定义

- hover/navigation factory 可独立挂载测试；无跨层 store/command import。
- 相关验收：AC-10、AC-11。
