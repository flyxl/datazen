# SQL 代码片段（SQL Snippets）设置与管理设计规范

- **日期**：2026-09-09
- **状态**：已批准 (Approved)
- **目标**：在 DataZen 设置界面的「编辑器」分栏中提供完备的代码片段查看、新增、编辑、删除、内置模板复制及导入导出能力，并将用户自定义片段无缝注入 SQL 编辑器自动补全链路。

---

## 1. 背景与现状

当前 DataZen 已在数据结构中定义了 `AppSettings.sqlSnippets`，并在 SQL 编辑器工具栏中提供了代码片段菜单按钮（`SnippetMenuButton.tsx`），同时支持在设置页底端通过剪贴板导入与导出编辑器配置。然而：
1. 用户在图形界面上无法直接浏览已有的内置与自定义 SQL 片段；
2. 无法直接在界面上对片段进行可视化新增、编辑或删除，必须手动拼装 JSON 字符串并通过剪贴板导入，操作体验割裂；
3. SQL 编辑器的 CodeMirror 自动补全源（`editorExtensions.ts`）当前默认仅挂载了内置片段，未完整将用户自定义片段传入补全引擎中。

---

## 2. 系统架构与模块职责划分

遵循单文件代码行数控制规范（<800行）与高内聚低耦合原则，不将复杂 UI 堆积在已有 600+ 行的 `SettingsContent.tsx` 中，而是拆分为两个专用组件：

```text
src/windows/settings/
├── SqlSnippetsCard.tsx       # 代码片段配置主卡片
└── SnippetEditDialog.tsx     # 片段新增与编辑弹窗
```

### 2.1 组件分工

1. **`SqlSnippetsCard.tsx`**：
   - 嵌入到 `SettingsContent.tsx` 的 `editor` 分组中。
   - 统一展示内置片段（`BUILTIN_SQL_SNIPPETS`）与用户自定义片段（`settings.sqlSnippets`）。
   - 顶部操作栏提供：新增片段按钮、配置导出（复制到剪贴板）、配置导入（从剪贴板读取并解析）。
   - 列表项清晰展示：快捷触发前缀（`<kbd>` 样式）、片段名称/描述、归属标记（内置 vs 自定义）、SQL 模板单行截断预览。
   - 提供操作：自定义片段可“编辑”和“删除（带二次确认）”；内置片段提供“复制为自定义模板”（方便基于官方模板修改）。

2. **`SnippetEditDialog.tsx`**：
   - 模态弹窗对话框（宽度约 540px），由 `SqlSnippetsCard` 调起。
   - 表单字段：
     - **触发前缀 (prefix)**：单行输入框，校验正则 `/^[A-Za-z_][A-Za-z0-9_*]*$/`。
     - **片段描述 (descriptionKey)**：单行输入框，输入纯文本描述（如 `按用户分组统计`）。
     - **SQL 模板正文 (template)**：多行等宽字体文本框（`font-mono text-xs min-h-[140px]`），支持 Tab 缩进。
   - 附带 CodeMirror 模板语法提示（展示 `${1:table_name}`、`${2:condition}` 等 Tabstop 规范与示例）。
   - 校验拦截：检查前缀与模板非空，校验前缀字符合法性，通过后回调保存。

3. **`SettingsContent.tsx`**：
   - 将原底部孤立的「配置导入与导出」区块升级替换为 `<SqlSnippetsCard />`。
   - 透传 `settings` 与 `updateField`。

4. **`SqlEditor.tsx` 与 `editorExtensions.ts`**：
   - `createCompletionExtensions` 接收完整的 `snippets?: readonly SqlSnippetItem[]`。
   - `SqlEditor.tsx` 中将内置片段与 `settings.sqlSnippets` 合并后传入，实现编辑器内输入前缀即时补全。

---

## 3. 详细设计与交互规范

### 3.1 数据契约

沿用 `src/types/index.ts` 中已有的数据定义：
```ts
export interface SqlSnippetItem {
  id: string;              // 唯一 ID，自定义片段使用 crypto.randomUUID() 生成
  prefix: string;          // 补全触发前缀，如 sel*, selw
  descriptionKey: string;  // 描述文本（内置片段为 i18n 键，用户片段为直接文本）
  template: string;        // 模板内容，遵循 CodeMirror snippet 语法
}
```

### 3.2 描述文本多语言与回退解析
`SnippetCompletionSource` 和 UI 列表展示描述时，先通过多语言函数尝试解析 `t(descriptionKey)`；若返回空或等于原 key，则直接回退显示 `descriptionKey` 本身。此逻辑确保中英日等环境下内置片段显示对应语言译文，而用户自定义输入的自由文本完整保留。

### 3.3 交互细节与防回归准则

1. **输入合法性保护**：
   - 前缀字段限制：必须符合 `/^[A-Za-z_][A-Za-z0-9_*]*$/`，防止输入空格或标点导致 CodeMirror 正则匹配失效。
   - 模板必填校验：禁止保存纯空模板。
2. **只读保护与衍生复制**：
   - 系统内置片段（`BUILTIN_SQL_SNIPPETS`）不提供删除和直接编辑入口，仅提供“复制为自定义”操作；点击后自动打开 `SnippetEditDialog` 并预填该模板内容与新生成的 UUID，由用户保存为新片段。
3. **安全删除**：
   - 删除自定义片段时调用 `useConfirmDialog` 弹窗确认，避免手滑误删。
4. **导入导出联动**：
   - 继续使用 `importEditorSettings` / `exportEditorSettings`（位于 `src/lib/settingsExport.ts`），导入时校验只更新白名单字段，成功/失败通过卡片内反馈状态即时告知。

---

## 4. 国际化与翻译 (i18n)

只在 `src/locales/en/query.ts` 和 `src/locales/zh-CN/query.ts` 中新增以下翻译项：
- `query.snippets.add`：Add Snippet / 新增片段
- `query.snippets.edit`：Edit Snippet / 编辑片段
- `query.snippets.duplicate`：Duplicate as Custom / 复制为自定义
- `query.snippets.builtin`：Built-in / 内置
- `query.snippets.custom`：Custom / 自定义
- `query.snippets.prefix`：Prefix / 快捷前缀
- `query.snippets.prefixPlaceholder`：e.g. selw / 例如 selw
- `query.snippets.prefixHint`：Letters, numbers, underscores and asterisk (*) / 允许字母、数字、下划线及星号
- `query.snippets.prefixInvalid`：Prefix must match /^[A-Za-z_][A-Za-z0-9_*]*$/ / 前缀格式不合法
- `query.snippets.description`：Description / 描述说明
- `query.snippets.descriptionPlaceholder`：e.g. Select with group by / 例如 按分组查询
- `query.snippets.template`：Template / SQL 模板
- `query.snippets.templatePlaceholder`：SQL snippet body / SQL 模板正文
- `query.snippets.templateRequired`：Template is required / 模板内容不能为空
- `query.snippets.syntaxGuide`：Use ${1:placeholder} for Tab stop navigation / 使用 ${1:占位符} 支持 Tab 键逐项跳转
- `query.snippets.deleteConfirmTitle`：Delete Snippet / 删除代码片段
- `query.snippets.deleteConfirmMessage`：Are you sure you want to delete snippet "{prefix}"? / 确定要删除代码片段 "{prefix}" 吗？
- `query.snippets.emptyCustom`：No custom snippets yet / 暂无自定义代码片段

---

## 5. 测试与验证方案

1. **单元测试**：
   - `src/windows/settings/__tests__/SnippetEditDialog.test.tsx`：
     - 测试合法性校验（空前缀、非法前缀字符、空模板拦截）。
     - 测试保存回调（提交有效数据与生成 UUID）。
   - `src/windows/settings/__tests__/SqlSnippetsCard.test.tsx`：
     - 测试列表渲染（内置片段与自定义片段展示）。
     - 测试操作触发（点击新增打开弹窗、点击复制预填弹窗、点击删除触发二次确认）。
     - 测试导入与导出按钮交互。
2. **编辑器补全测试**：
   - 验证向 `SqlEditor` 注入自定义片段时，`createSnippetCompletionSource` 正确包含自定义项。
3. **回归测试**：
   - 运行现有的 `settingsExport.test.ts`、`snippetJourney.test.ts`、`SettingsContent.test.tsx` 确保 100% 通过。
