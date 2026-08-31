# v01x-ai-actions Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-ai-actions-BUG-001 | [S2] 凭据别名 `apiToken`/`authToken`/`oauthToken` 未被脱敏 | 待验证(修复后) | 见下方 | 2026-08-31 修复轮已覆盖文本与嵌套 schema 回归，待全新测试代理复验 |
| v01x-ai-actions-BUG-002 | [S2] `queryResult` 结果集别名未被移除，行数据进入 AI prompt context | 待验证(修复后) | 见下方 | 2026-08-31 修复轮已覆盖结果别名与 1000→100 上限回归，待全新测试代理复验 |

## BUG-001：凭据字段别名未完全脱敏

- **记录时间**：2026-08-31
- **重现步骤**：调用 `buildQueryDiagnosisContext`，传入 `schemaContext: { apiToken: 'API_TOKEN', authToken: 'AUTH_TOKEN', oauthToken: 'OAUTH_TOKEN' }`；或调用 `redactSensitiveText('apiToken=API_TOKEN authToken: AUTH_TOKEN')`。
- **预期结果**：明显凭据字段和值不出现在 `promptContext`、`schemaContext` 或脱敏文本中。
- **实际结果**：`apiToken`、`authToken`、`oauthToken` 字段和值原样保留；对应 key/value 文本也原样保留。
- **影响**：AI prompt 可能携带 API/token 凭据别名，违反 G 轨道“AI Prompt 不包含凭据、AI Key”的验收要求。
- **修复结果**：凭据 key 按驼峰、大小写和分隔符拆词识别；文本中的完整敏感键值对和 schema 中的敏感字段均不进入 prompt。
- **验证记录**：`npx vitest run src/lib/__tests__/aiQueryActions.test.ts` 10/10 通过；聚焦 tsc 通过，待独立测试代理复验。

## BUG-002：结果集字段别名未完全过滤

- **记录时间**：2026-08-31
- **重现步骤**：调用 `buildQueryDiagnosisContext`，传入 `schemaContext: { queryResult: Array.from({ length: 1000 }, (_, i) => ({ id: i, email: 'user-' + i + '@example.test' })) }`。
- **预期结果**：结果行不进入 `promptContext.schemaContext`；大结果集应被整体移除或采用明确的安全摘要。
- **实际结果**：`promptContext.schemaContext.queryResult` 仍存在，1000 行被截为 100 行，首行数据仍为 `{ id: 0, email: 'user-0@example.test' }`，序列化结果约 4.1 KB。
- **影响**：非 `rows`/`results` 等白名单命名的结果集仍会进入 AI prompt；截断数量不等于不发送结果数据。
- **修复结果**：`queryResult`、`queryResults`、`result`、`results`、`resultSet` 及其分隔符变体统一过滤；其他递归数组继续受既定 100 项上限约束。
- **验证记录**：`npx vitest run src/lib/__tests__/aiQueryActions.test.ts` 10/10 通过；聚焦 tsc 通过，待独立测试代理复验。
