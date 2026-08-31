# v01x-ai-actions 进度

## 1. 功能摘要

- 编号：v01x-ai-actions
- 范围：QueryDiagnosisContext 脱敏、Explain/Fix SQL/Retry 纯 action descriptor 与单元测试
- 状态：独立复测通过·待测试提交；BUG-001/002/003/004 修复均通过回归
- 编码 commit：`63d8e6dfe49d299205899b88c7b0561330dd0b5b`、`49058f8b1da2bdf39ab0540e20d594f72f2ebe90`、`bbd7c4cc70b7d43000b85372f28eb92b67568818`
- 测试验收：全新复测代理对三个编码提交的累计结果执行定向/相关 Vitest、模块严格 tsc、静态 adversarial review 与 `git diff --check`；BUG-003/004 未发现新的明确业务 bug，Explain/Fix SQL/Retry 主路径与无副作用边界通过，I 轨接线和 E2E 留待后续

## 2. E2E 用例登记

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| AI-ACTIONS-E2E-001 | 查询失败后 Explain 使用脱敏 SQL/error 与当前 database/session context | 留待 R 回归 | 本轨不接 QueryErrorPanel；待 I 接线后执行 |
| AI-ACTIONS-E2E-002 | Fix SQL 展示原 SQL/diff，Apply to Editor 只更新草稿，不执行数据库命令 | 留待 R 回归 | 本轨无 UI；待 I 接线后执行 |
| AI-ACTIONS-E2E-003 | Retry 使用当前 SQL/绑定参数；切换 database/schema/session 后阻止重试并提示 context changed | 留待 R 回归 | 本轨无 UI；待 I 接线后执行 |

## 3. 测试结果与覆盖率

- 定向 Vitest：`npx vitest run src/lib/__tests__/aiQueryActions.test.ts`，1 个文件、12 个测试通过。
- 相关 AI Vitest：`npx vitest run src/lib/__tests__/aiQueryActions.test.ts src/components/ai/__tests__/DiagnosisPanel.test.tsx src/stores/__tests__/aiStore.test.ts`，3 个文件、55 个测试通过。
- 独立 adversarial review：运行时断言覆盖 `resultsSet`/`resultsRows`/`resultsData` 的大小写、camelCase、`_`/`-`/`.`/重复分隔符变体；`resultStatus`、`resultsSummary`、`businessResults`、`dataPoints`、`resultCount`、`resultType` 等普通业务字段保留；嵌套 JSON 结果别名移除；含转义引号、反斜杠、换行的 token/password 无片段残留；结果数组与普通数组均严格截取 100 项；通过。
- Descriptor review：Explain 构建不回调、invoke 只传既有 `ai_diagnose_error` 脱敏参数；Fix 构建不写 editor，`applyToEditor` 只回调 draft 且保留 original SQL/diff；Retry 仅在 context fingerprint、SQL、bound params 全部匹配后回调，context/SQL/params 任一变化均拒绝；模块无 IPC/数据库命令导入；通过。
- 聚焦覆盖率：`npx vitest run --coverage --coverage.include=src/lib/aiQueryActions.ts --coverage.reportsDirectory=/private/tmp/datazen-v01x-ai-actions-coverage-fix-20260831 src/lib/__tests__/aiQueryActions.test.ts`，`aiQueryActions.ts` lines 95.18%、statements 87.50%、branches 75.47%、functions 97.43%，命令通过；默认全局 coverage 阈值会将未执行的其他目录计为 0，未作为本轨判定依据。
- 本轨模块严格类型检查：`npx tsc --noEmit --strict --target ES2022 --module ESNext --moduleResolution bundler --skipLibCheck --allowImportingTsExtensions src/lib/aiQueryActions.ts src/lib/__tests__/aiQueryActions.test.ts`，通过。
- `npx tsc --noEmit`：按复测前置要求运行 `node scripts/generate-builtin-locales.mjs` 生成 ignored 的 `src/locales/builtinLocales.ts` 后通过；生成物未纳入提交。
- Rust host 回归：`CARGO_TARGET_DIR=/private/tmp/datazen-v01x-ai-actions-target cargo test -p datazen --lib`，1193 通过、0 失败、2 ignored；本 commit 无 Rust 改动。
- `git diff --check`：记录更新后通过。
- 静态/动态边界核对：模块无 IPC/数据库命令导入，无 AskQuestion→Workflow 实现；Fix build/apply 仅生成/回调 editor draft，Retry 在 callback 前校验 context fingerprint、SQL 和 bound params；本轮仅收紧脱敏/结果集过滤，descriptor 语义未变。
- 修复边界：结果键按大小写不敏感的完整分词别名匹配，覆盖 `result(s)Set/Rows/Data`、`queryResult(s)*` 及既有别名，并保留 `resultStatus`、`resultsSummary`、`businessResults` 等非完整别名字段；JSON 文本先解析并递归移除敏感/结果字段，再由转义感知扫描处理非 JSON 文本。
- 未修改 `QueryErrorPanel.tsx`、`QueryPanel.tsx`、`panelStore.ts`、locales、hub、规格文档或 tracked codegen 文件；按要求生成的 `src/locales/builtinLocales.ts` 保持 ignored 且不纳入本提交；`hub.md` 的既有工作区变更原样保留且不纳入本提交。

## 4. 设计决策 / 遗留

- `buildQueryDiagnosisContext` 是唯一上下文入口；要求 SQL、error、connectionId、dbSessionId、databaseType、database 完整，schema 可为空。
- AI 诊断参数使用脱敏 SQL/error；Fix SQL 和 Retry 仍保留原 SQL。schema context 采用深度/条数/字符串上限，并按统一 key 识别覆盖 token/key/secret/password/credential 变体及 queryResult/result/results 结果集别名。
- Retry descriptor 只向调用方回调传递当前 SQL、原始 bound params 和 context fingerprint；本模块不调用 query/AI IPC，也不生成 Workflow。
- 独立测试代理、I 轨接线和真实桌面 E2E 留待后续；E2E 按 playbook 登记为 R 回归。
