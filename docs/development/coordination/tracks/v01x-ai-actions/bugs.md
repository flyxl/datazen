# v01x-ai-actions Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-ai-actions-BUG-001 | [S2] 凭据别名 `apiToken`/`authToken`/`oauthToken` 未被脱敏 | 待验证 | 见下方 | 2026-08-31 独立测试轮动态复现，待修复后复验 |
| v01x-ai-actions-BUG-002 | [S2] `queryResult` 结果集别名未被移除，行数据进入 AI prompt context | 待验证 | 见下方 | 2026-08-31 独立测试轮动态复现，待修复后复验 |

## BUG-001：凭据字段别名未完全脱敏

- **记录时间**：2026-08-31
- **重现步骤**：调用 `buildQueryDiagnosisContext`，传入 `schemaContext: { apiToken: 'API_TOKEN', authToken: 'AUTH_TOKEN', oauthToken: 'OAUTH_TOKEN' }`；或调用 `redactSensitiveText('apiToken=API_TOKEN authToken: AUTH_TOKEN')`。
- **预期结果**：明显凭据字段和值不出现在 `promptContext`、`schemaContext` 或脱敏文本中。
- **实际结果**：`apiToken`、`authToken`、`oauthToken` 字段和值原样保留；对应 key/value 文本也原样保留。
- **影响**：AI prompt 可能携带 API/token 凭据别名，违反 G 轨道“AI Prompt 不包含凭据、AI Key”的验收要求。

## BUG-002：结果集字段别名未完全过滤

- **记录时间**：2026-08-31
- **重现步骤**：调用 `buildQueryDiagnosisContext`，传入 `schemaContext: { queryResult: Array.from({ length: 1000 }, (_, i) => ({ id: i, email: 'user-' + i + '@example.test' })) }`。
- **预期结果**：结果行不进入 `promptContext.schemaContext`；大结果集应被整体移除或采用明确的安全摘要。
- **实际结果**：`promptContext.schemaContext.queryResult` 仍存在，1000 行被截为 100 行，首行数据仍为 `{ id: 0, email: 'user-0@example.test' }`，序列化结果约 4.1 KB。
- **影响**：非 `rows`/`results` 等白名单命名的结果集仍会进入 AI prompt；截断数量不等于不发送结果数据。
