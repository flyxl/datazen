# Brief: sql-s5-c — 统一风险确认与 QueryPanel 接线

> 依赖：S5-A、S5-B 已合流；S2-D（risk classifier）；S1-D（i18n key，只消费不新增）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S5-C + §3.1/§3.3（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s5-c/progress.md`。

## 独占写锁

可建/可改：`src/windows/connection/query/useQueryExecutionGate.ts`、QueryPanel 薄接线、
query execution contracts、`src/components/ui/ConfirmDialog.tsx`、
`src/hooks/useConfirmDialog.tsx` 及 tests。
**禁止**：locale（只用 S1-D 已定义 key）、leaf 算法。

## 步骤

1. toolbar/shortcut/gutter/all/retry/unclosed-transaction continue 统一经 execution gate；
   EXPLAIN/事务控件/导出/AI Apply 按 §3.1 白名单排除。
2. gate 冻结 target/context/params；先事务检查+参数校验，再按
   readOnly→Safe Mode hard block→production/high-risk confirm 顺序处理；
   最终只调 `submitExecution(snapshot)`。
3. production 规则：`isProduction && classification !== 'read'`；
   危险+production 合并一次最高级确认并列全部 reason。
4. 通用 ConfirmDialog 只加 optional badge/codePreview/description；旧调用视觉行为不变。
5. SQL 预览限行数/字符数，可复制完整 target；不显示参数历史或连接凭据。
6. 取消/焦点退出/document revision/SQL/panel/connection/session/group/readOnly/safeMode/
   参数 fingerprint 任一改变 → cancelled 或 stale，不得提交。
7. Safe Mode 后端错误保持可解释提示，不提供绕过按钮。

## 测试

readOnly、Safe Mode on/off、no-WHERE、production 普通写/高危/unknown、SELECT、
高危+生产单弹窗、选区/当前/gutter/all、确认期间变化、unclosed transaction、
snapshot submit、failed snapshot Retry、五参数真实 UI→Host。

## 完成定义

- 不存在绕开 gate 的执行入口；Host guard 测试继续通过。
- 相关验收：AC-15（UI→Host）、AC-16 部分（Retry 无回归）、AC-17。
