# Brief: sql-s5-b — BindParamPanel 升级与参数历史

> 依赖：S2-C 已合流（Rust binder / v2 wire contract 已冻结，只消费不改）。
> **不修改** QueryPanel、execution gate、Rust。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S5-B + §6.7（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s5-b/progress.md`。

## 独占写锁

可建/可改：`src/components/query/BindParamPanel.tsx`、
新 `src/windows/connection/query/useBindParameters.ts`、`src/lib/sqlBindParams.ts`、
历史 UI 及 tests。
**禁止**：QueryPanel、execution gate、Rust binder。

## 步骤

1. `sqlBindParams.ts` 由 `:name`/`$1` 扩展至五类 syntax，生成 v2 wire payload 的稳定 ID + 精确 occurrence 映射。
2. 面板按当前 execution target 解析 descriptor；命名参数去重、question 保持 occurrence；突出活动语句内参数。
3. label 保留原 syntax，input key 用稳定 ID；目标变化时复用仍存在的值。
4. 历史下拉：鼠标/上下键/Enter/Esc；最近 5 条 + 可清除；localStorage 不可用/配额错误/损坏 JSON 静默降级。
5. hook 输出 `buildPayloadForTarget(target.sql)`、`markSubmitted(snapshot)`、参数 fingerprint，供 S5-C 消费；本轨不接执行入口。
6. `markSubmitted` 记录历史，敏感名（password/token/secret/key/credential）不记录；只有 S5-C 在真正提交前调用。
7. 紧凑横条；不增加模态对话框；不支持 raw SQL 值。

## 测试

五语法展示、shared name、question ordinal、历史隔离/键盘/a11y、失败降级、执行目标切换、Retry 参数改变取消。

## 完成定义

- 参数 UI/hook 可独立测试并生成符合 v2 contract 的 payload；真实 UI→Host 验收由 S5-C 完成。
- 相关验收：AC-15 部分。
