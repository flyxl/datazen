# Brief: sql-s3-b1 — ContentView / PanelContentRenderer 等价拆分

> 依赖：无（纯等价拆分，可与 S3-A、S3-C 同 wave）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S3-B1（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s3-b1/progress.md`。

## 独占写锁

可建/可改：`src/windows/connection/ContentView.tsx`、
`src/windows/connection/PanelContentRenderer.tsx` 及新拆出的 workspace/view 子组件。
**禁止**：QueryPanel、AiChatPanel、任何行为变更（纯等价拆分）。

## 步骤

1. 抽取多模态对话框容器（创建库/模式/用户、执行 SQL 文件、导入导出等弹窗 → 独立 Dialogs 承载组件）。
2. 抽取右侧抽屉容器与状态协调（AI 助手抽屉 `aiChatOpen` 与数据详情抽屉 `detailRow`）。
3. 保持 table/data/structure navigation、split layout、active panel 生命周期。
4. 两个 touched 生产文件及新模块全部降到 500 行以下。

## 测试

现有 ContentView / PanelContentRenderer 测试、导航、AI sidebar、panel switching 回归。

## 完成定义

- 纯等价拆分通过；新 bridge 有稳定的窄接入点供 S3-B2 消费。
