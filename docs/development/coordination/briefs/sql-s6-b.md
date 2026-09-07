# Brief: sql-s6-b — INSERT Hint 设置持久化

> 依赖：S4-C（hint extension 接受 setting boolean；动态装卸仍由 S6-D 测）。
> 翻译只消费 S1-D 已建立 key，**不修改** locale。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S6-B（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s6-b/progress.md`。

## 独占写锁

可建/可改：AppSettings TS/Rust、settings store/UI/tests。
**禁止**：locale、leaf 算法。

## 步骤

1. 新增持久化字段 `editorInsertValueHints`，默认 true；Rust serde default 兼容旧配置。
2. 现有编辑器设置区加 switch；描述只影响视觉提示、不修改 SQL。
3. 旧配置缺字段按 serde/default 正常加载；保存后字段稳定。
4. 本轨只验证 setting/prop；关闭时不装配、重开即时生效由 S6-D composer 测试。

## 测试

旧配置迁移、默认值、store save/load、Settings switch、编辑器 reconfigure。

## 完成定义

- 设置跨重启保存；前后端配置类型和默认值通过。
- 相关验收：AC-09 部分。
