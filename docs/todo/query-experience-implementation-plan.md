# 实施方案：DataZen 全景查询体验（Query Experience）增强

> 对应 PRD：[`docs/todo/query-experience-prd.md`](./query-experience-prd.md)
> 开发轨道：`feat/query-experience`（Host worktree `datazen-qx/` + Pro 嵌套 worktree）

---

## 0. 基线勘察结论（与 PRD 的差异修正）

在编码前对现网代码做了完整勘察，PRD 中有 4 处与现状不符，本方案按实际代码修正：

| # | PRD 描述 | 实际代码现状 | 方案处置 |
| :-- | :--- | :--- | :--- |
| D1 | 轨道 B 需实现 S4-A/S4-B/S4-D/S4-E 四项 | `packages/pro-extensions/sql-editor-pro` 已实现 `statement-gutter/`（S4-A）、`join-completion/`（S4-B）、`hover/`（S4-D） | **轨道 B 收敛为仅 §7.1 Linter（S4-E）** |
| D2 | `SqlEditorHandle` 只需改 `packages/extension-points/src/sql-editor/contracts.ts` | 该接口有**两份镜像**：`src/components/sql-editor/contracts.ts`（宿主实际使用）与 EP 包内同名文件（供 Pro 跨仓类型引用） | **两份同步扩充**，否则 Pro 侧类型不可见 |
| D3 | §4.4 Paste as IN / Drop Caret 归属 Host 基础层 | 现由 Pro `createPasteExtensions` 独占实现；Community 版完全没有该能力 | **Host 新增兜底实现**；Pro 激活时仍优先用 Pro 版（保持 §3.1 契约不变，非破坏性） |
| D4 | §5.2 参数拦截只需比对 `paramValues` | `sqlParams`/`paramValues` 全部来自 **Pro 的 `useBindParameters`**；Community 版 `params` 恒为 `[]`，门闸的参数校验分支永远不触发，`substituteSqlParams` 仍会把未赋值参数替换成 `NULL` | **门闸改为 Host 侧 `parseSqlParams` 自主解析**，不依赖 Pro 状态，Community/Pro 双版本一致拦截 |

D4 是本次最关键的安全修复：现有 `useQueryExecutionGate.ts` 的校验虽已存在，但数据源错位导致在 Community 版形同虚设。

---

## 1. 轨道划分与工作目录

| 轨道 | 仓库 | 工作目录 | 分支 |
| :-- | :--- | :--- | :--- |
| A：Host 宿主 | `flyxl/datazen` | `/Users/wuxiaolong/code/rust-projects/datazen-qx` | `feat/query-experience` |
| B：Pro 插件 | `flyxl/datazen-extension-sql-editor-pro` | `datazen-qx/packages/pro-extensions/sql-editor-pro`（嵌套 worktree） | `feat/query-experience` |

Pro 目录在宿主仓库中是 `gitignored` 的独立 Git 仓库，因此以**嵌套 worktree** 方式挂载：两条轨道各自独立提交，互不污染。

---

## 2. 分阶段实施计划

### Phase 1 — EP 契约演进（非破坏性）

| 落点 | 变更 |
| :--- | :--- |
| `src/components/sql-editor/contracts.ts` | `SqlEditorHandle` 增加可选 `formatDocument?()`、`insertSnippet?(template)` |
| `packages/extension-points/src/sql-editor/contracts.ts` | 同步镜像上述两个可选方法 |
| `packages/extension-points/src/sqlEditorProEP.ts` | `SqlEditorProFeatures` 增加 `createLinterExtensions?(opts, refs)`；`fallbackFeatures` 补 `createLinterExtensions: () => []` |

全部为**可选成员**，Pro 旧版本不实现也能编译，满足 PRD §3 向后兼容要求。

### Phase 2 — 模块一：编辑器核心与人体工学（Host）

**2a. Snippets 模版系统（§4.1）**
新建 `src/components/sql-editor/snippets/`：
- `types.ts` — `SqlSnippetItem { id, prefix, label, description, template }`
- `builtinSnippets.ts` — 内置 7 个高频模版（`sel*` / `selc` / `ins` / `upd` / `del` / `join` / `count`）
- `snippetCompletion.ts` — 用 `@codemirror/autocomplete` 的 `snippetCompletion` 包装为 `CompletionSource`，挂进 `createCompletionExtensions` 的 `override` 数组

关键约束（遵循 `.cursor/rules/interaction-and-testing-principles.mdc`）：Snippet 补全源与 Schema/Keyword 源**并行共存**，用 `boost` 软排序而非硬过滤；在 `kind === 'table'` 等明确互斥上下文降权而不 `return null`。

**2b. 可配置 Beautify（§4.2）**
- `src/types` 的 `AppSettings` + `settingsStore` 的 `DEFAULT_SETTINGS` 增加 `sqlFormatOptions: SqlFormatOptions`
- 重构 `src/lib/sqlFormat.ts`：`formatSql(sql, databaseType, options?)` 把 `keywordCase` / `indentStyle` / `breakBeforeBooleanOperators` / `linesBetweenQueries` 映射到 `sql-formatter`；保持旧签名默认值不变以免影响 7 处现有调用方
- 新增 `src/lib/formatEditorDocument.ts`：`formatEditorDocument(view, options)` — 有选区只格式化选区并保留选区；无选区格式化全文并按字符偏移比例回锚光标
- `Shift-Alt-f` 快捷键绑进 `createBaseEditorExtensions`

**2c. 命令式句柄（§4.3）**
`SqlEditor.tsx` 的 `useImperativeHandle` 挂载 `formatDocument`（调用 2b 的引擎，从 settingsStore 读配置）与 `insertSnippet`（`@codemirror/autocomplete` 的 `snippet()` 在光标处展开并激活 Tabstop）。

**2d. 顶部工具栏（§4.3）**
`QueryEditorSection.tsx` 当前 418 行，已接近 800 行上限，因此**抽出子组件**而非就地膨胀：
- 新建 `src/windows/connection/query/toolbar/SnippetMenuButton.tsx` — 片段下拉
- 新建 `src/windows/connection/query/toolbar/RefreshCompletionButton.tsx` — 调 `metadataCache.invalidateSession(dbSessionId)` + 成功 Toast
- 现有【格式化】按钮的 `onClick` 从 `onFormat`（整篇 `formatSql` 替换）改为优先 `editorRef.current?.formatDocument()`，Tooltip 补快捷键

**2e. Paste as IN / Drop Caret Host 兜底（§4.4，修正 D3）**
新建 `src/components/sql-editor/paste/hostPasteAsIn.ts` + `parseDelimitedValues.ts`：解析制表符/换行/逗号分隔剪贴板，转义单引号，生成 `('a', 'b')`。在 `createPasteExtensions` 中：Pro 返回非空则用 Pro，否则用 Host 兜底。

### Phase 3 — 模块二：执行门禁与执行监控（Host）

**3a. 执行策略四模态（§5.1）**
- `AppSettings` 增加 `sqlExecutionStrategy: 'current_statement' | 'entire_script' | 'largest_statement' | 'ask'`（默认 `current_statement`）
- 新建 `src/windows/connection/query/resolveExecutionTarget.ts`：纯函数，输入 `(doc, cursor, selection, strategy)`，输出 `{ sql, from, to, statementCount }`。优先级 `Selection > Strategy`
- `useQueryExecutionGate` 在 `requestExecute` 最前置调用；`ask` 模态弹 ActionSheet 让用户选"当前语句 (Line X-Y)"或"全脚本 (共 N 条)"
- 工具栏加策略切换器

**3b. 未赋值参数拦截（§5.2，P0 Bug，修正 D4）**
- 新建 `src/windows/connection/query/validateBindParams.ts`：`findMissingParams(sql, values, policy)` 纯函数，基于 Host 的 `parseSqlParams` 自主解析，不依赖 Pro
- `useQueryExecutionGate` 的第 2 步改用它；命中缺失时**阻断执行**、报出缺失 token 列表、`document` 派发事件请求展开 BindParamPanel 并聚焦首个缺参输入框
- `runExecute` 中 `substituteSqlParams` 之前再加一道断言，确保任何路径都不会静默送 `NULL`

**3c. 多语句分段耗时（§5.3）**
按 Driver Command API 分层落地（具体落点依勘察结果，见 §4 风险）：Rust 侧在多语句循环中记录每段 `Instant`，`QueryResult` 增加 `statement_timings: Vec<StatementTiming>`（`index` / `sql_snippet` / `elapsed_ms` / `affected_rows`），TS 类型同步；结果栏总耗时旁增加可展开的分段耗时列表。字段为 `Option`/默认空数组，未升级的驱动优雅退化。

### Phase 4 — 模块三：轻资产中心（Host）

- **6.1 收藏直达新 Tab（P0）**：收藏项主动作改为 `panelStore` 开新 QueryPanel 并注入 SQL、以收藏标题作 Tab 名、聚焦编辑器
- **6.2 历史全文检索（P1）**：历史抽屉加即时搜索框（SQL 关键字 / 时间范围 / 库名）+ "当前连接 / 全部连接"范围标识
- **6.3 结果 Tab Pin（P1）**：结果 Tab 加 📌，Pin 过的 Tab 在下次执行时不被回收，强制新开 Result Tab
- **6.4 导入导出（P2）**：设置面板 `Export Settings & Snippets (JSON)` / `Import`

### Phase 5 — 轨道 B：Pro 写时静态波浪线诊断（§7.1）

`packages/pro-extensions/sql-editor-pro/src/linter/`：
- `createLinterExtensions.ts` — `@codemirror/lint` 的 `linter(source, { delay: 300 })`
- `unknownTableDiagnostic.ts` — 从 `modelRef` 取 `FROM`/`JOIN`/`UPDATE`/`INTO` 表标识符，白名单过滤 CTE 别名、`DUAL` 等虚拟表、`#temp` 临时表，与 `metadataSnapshotRef` 比对
- `unknownColumnDiagnostic.ts` — 仅在有确定限定符（`u.col`）时触发；裸列名不报错
- 降级：文档 > 5,000 行或 > 100KB 时仅诊断光标 ±200 行
- 在 `proFeatures.ts` 挂载 `createLinterExtensions`
- Host 侧 `editorExtensions.ts` 新增 `createLinterCompartmentExtensions` 并接入 `compartments` 优先级链

### Phase 6 — 验收

按 PRD §9 八项门禁逐条验证；每阶段配套 Vitest 单测（含 §4.1/§5.1 的**连续旅程测试**：模拟击键全过程断言状态跃迁与退出）；最后跑 Community + Pro 双版本编译。

---

## 3. 测试策略

| 范围 | 位置 | 命令 |
| :--- | :--- | :--- |
| Host 前端单元 | `src/**/__tests__/` | `npx vitest run` |
| Pro 插件单元 | `packages/pro-extensions/sql-editor-pro/src/**/__tests__/` | `pnpm test:pro` |
| Host Rust 单元 | `src-tauri/src/**` 内 `#[cfg(test)]` | `cargo test -p datazen --lib` |
| 双版本编译 | — | `pnpm build`（Community）/ `pnpm build:pro` |

重点测试类型：
1. **连续旅程测试**：Snippet 展开 → Tab 前进 → Shift-Tab 回退；执行策略在光标位于空白行/语句间的跃迁
2. **安全回归测试**：断言未赋值参数场景下**没有任何 IPC 调用发生**（而非只断言弹窗出现）
3. **性能门禁**：5,000 行脚本的 Linter 降级路径与击键 CPU 预算

---

## 4. 已识别风险

| 风险 | 影响 | 缓解 |
| :--- | :--- | :--- |
| §5.3 分段耗时需改 Driver Command API 契约 | 涉及 Rust + 所有驱动，改动面最大 | 新字段一律 `Option` / 默认空数组，驱动未升级时前端隐藏分段视图 |
| `QueryEditorSection.tsx` 418 行、`QueryPanel.tsx` 571 行 | 逼近 800 行上限 | 新 UI 一律抽子组件到 `query/toolbar/`，不就地膨胀 |
| Snippet 补全源与既有 5 个补全源竞争排序 | 可能挤掉表名/列名补全 | 只用 `boost` 软排序，配旅程测试断言各上下文首选项 |
| Pro `createPasteExtensions` 与 Host 兜底重复注册 | 双份 keymap 冲突 | Host 兜底仅在 Pro 返回空数组时启用 |
| §6.4 导入导出触达凭据 | 泄露风险 | 导出白名单字段，显式排除连接凭据与 AI 密钥 |

---

## 5. 交付顺序

Phase 1 → 2 → 3 → 5 → 4 → 6。

Phase 5（Pro Linter）提前到 Phase 4 之前，因为它依赖 Phase 1 的 EP 契约，且是 PRD §9 验收表中唯一的 Pro 项；Phase 4 资产中心与编辑器核心解耦度最高，放最后不阻塞其他阶段。
