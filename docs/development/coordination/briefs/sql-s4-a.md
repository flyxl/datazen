# Brief: sql-s4-a — Statement Frame、Gutter 与执行状态

> 依赖：S2-A（range contract）、S2-B（scope，可选）。
> 本阶段只交付可独立测试的 CodeMirror extension factory 和纯逻辑模块，
> **不修改**中央 `SqlEditor.tsx` / `editorExtensions.ts`（由 S6-D 唯一装配）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S4-A + §6.1（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s4-a/progress.md`。

## 独占写锁

可建/可改：statement frame / gutter extension factory 及其测试。
**禁止**：SqlEditor composer、QueryPanel。

## 步骤

1. frame 只消费统一 active statement range；多行非空 selection 时隐藏。
2. 超 500 行停止昂贵几何探测并隐藏 frame，只保留 gutter；设可测试的 degraded state。
3. gutter 只在每条有效语句首个可执行行显示 play；纯注释不显示。
4. tooltip 文案区分 macOS 与其他平台快捷键。
5. 点击 marker 生成 `SqlExecutionTarget(source='gutter')`。
6. running spinner 按 documentVersion + targetRange 精确匹配；取消/结束恢复 play。
7. 用主题 token 和 Web tooltip，不引入 Tauri menu。

## 测试

range 映射、selection hide、500 行降级、marker click、running/cancel、文档改变、暗色/亮色 class。
jsdom 不断言真实像素布局；几何计算抽纯函数。

## 完成定义

- Frame、gutter、快捷键对同一 SQL 产生同一 range（复用同一 ranges，不各写一套）。
- 本阶段 DoD 是 factory contract 通过，不宣称用户可见。
- 相关验收：AC-01、AC-02。
