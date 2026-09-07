# Brief: sql-s4-c — Intention 与 INSERT Hint

> 依赖：S2-B（semantic reference）、S3-A（metadata 物理列顺序，可选降级）。
> 只交付 factory + 纯/extension tests，**不修改** composer。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S4-C + §6.3/§6.4（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s4-c/progress.md`。

## 独占写锁

可建/可改：`src/components/sql-editor/intentions/`、insertHints 及纯/extension tests。
**禁止**：composer、locale。

## 步骤

1. `analyzeIntentions` 返回纯 action 数据，`applyIntention` 单 transaction 应用。
2. `SELECT *`：单 relation 扩展物理列；`alias.*` 只扩对应 relation；
   多 relation 裸 `*` 策略不唯一则不提供动作（不展开 `COUNT(*)`/乘法/字符串星号）。
3. add/remove qualifier 只处理 semantic reference token，不用全文字符串替换；冲突或歧义拒绝。
4. Alt+Enter 菜单支持键盘、Esc、focus 恢复、lightbulb 入口。
5. INSERT 有显式列本地对位；无显式列用 metadata 物理顺序；多 VALUES 行均提示；
   列数不匹配只提示可确定部分；不对 INSERT...SELECT / DEFAULT VALUES 伪造 hint。
6. inlay 只分析 viewport ∩ active statement + debounce；设置关闭时不装配。
7. extension 接受 setting boolean；动态装卸由 S6-D composer 完成。

## 测试

intention 事务和 cursor、歧义拒绝、多 VALUES/viewport、setting on/off factory 行为。

## 完成定义

- intention/hint factory 在缺 metadata 时优雅缺席，不错误改写 SQL。
- hint 是 decoration，不进入复制/搜索/undo/SQL 请求。
- 相关验收：AC-08、AC-09 部分（持久化由 S6-B 做）。
