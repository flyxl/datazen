# Brief: sql-s5-a — Paste as IN、Drop Caret 与多光标

> 依赖：S2-B（dialect adapter 的 quote 逻辑）、S3-C（drag payload contract）。
> extension 仍是 leaf，由 S6-D 装配；**不修改** tree / SqlEditor composer。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S5-A + §6.6（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s5-a/progress.md`。

## 独占写锁

可建/可改：paste / drop / multiple-selection extension factory、context-menu item factory 及测试。
**禁止**：tree、SqlEditor composer。

## 步骤

1. 纯 `parseDelimitedValues`：CRLF/newline/comma/tab、quoted delimiter、trim、空项策略。
2. "自动类型" vs "全部按字符串"两种模式：数字保持数值、`NULL` 保持 SQL NULL；
   其余单引号并 `'`→`''` 转义；不执行表达式。右键子项选择；快捷键用上次选择或默认自动类型。
3. source 上限 1 MiB、value 上限 10,000；超限拒绝 + 可本地化提示（只用 S1-D 已定义 key）。
4. 光标前是 `IN`/`NOT IN` 只插括号，否则插 `IN (...)`；非空 selection 替换 selection。
5. 绑定 `Mod+Shift+V` 和 Web Context Menu 项；clipboard 读取失败不改文档。
6. drop caret：校验 source connection；按 plan §4.6 决定 empty/non-empty/table/column 行为。
7. quote 用 dialect adapter；PG 全小写安全标识符可免引号，保留字/大小写/特殊字符必须引用。
8. 显式启用 `allowMultipleSelections`、Mod+D next occurrence、`rectangularSelection()`，处理 keymap 优先级。

## 测试

delimiter/quote/NULL/numeric/limits/prefix、clipboard failure、drop caret 生命周期、
legacy/new payload、cross-connection、三 quote style、Mod+D 和矩形 selection。

## 完成定义

- 所有写文档行为单 transaction，undo 一次完整撤销。
- 相关验收：AC-12、AC-13、AC-14。
