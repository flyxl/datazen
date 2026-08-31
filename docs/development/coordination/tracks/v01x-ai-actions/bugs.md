# v01x-ai-actions Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-ai-actions-BUG-001 | [S2] 凭据别名 `apiToken`/`authToken`/`oauthToken` 未被脱敏 | 已验证 | 见下方 | 2026-08-31 独立测试复验通过：文本与嵌套 schema 均不含测试凭据 |
| v01x-ai-actions-BUG-002 | [S2] `queryResult` 结果集别名未被移除，行数据进入 AI prompt context | 已验证 | 见下方 | 2026-08-31 独立测试复验通过：结果别名被移除，普通数组保留 100 项上限 |
| v01x-ai-actions-BUG-003 | [S2] 复数结果集别名 `resultsSet`/`resultsRows`/`resultsData` 未被过滤 | 待修复 | 见下方 | 2026-08-31 独立测试复验复现，修复建议见下方 |
| v01x-ai-actions-BUG-004 | [S2] JSON 转义引号导致敏感值尾部残留在脱敏文本中 | 待修复 | 见下方 | 2026-08-31 独立测试复验复现，修复建议见下方 |

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

## BUG-003：复数结果集别名未完全过滤

- **记录时间**：2026-08-31
- **重现步骤**：调用 `buildQueryDiagnosisContext`，传入 `schemaContext: { resultsSet: Array.from({ length: 1000 }, (_, i) => ({ id: i, email: 'user-' + i + '@example.test' })) }`；也可替换为 `resultsRows`、`resultsData` 或全小写 `resultsset`。
- **预期结果**：所有结果集别名都不进入 `promptContext.schemaContext`；结果行不应仅依靠数组截断来限制发送量。
- **实际结果**：`resultsSet`、`resultsRows`、`resultsData`、`resultsset` 字段仍存在于 sanitized schema；数组被截为 100 项，但行数据仍进入 AI prompt context。
- **影响**：业务调用方使用复数 camelCase/大小写变体时，查询结果数据可能被发送给 AI，违反结果集不绕过上限的安全要求。
- **建议**：将 `result`/`results` 的后续 `set`、`rows`、`data` 组合按同一大小写与分隔符规则统一匹配，补充复数组合及大小写变体回归测试。

## BUG-004：转义引号会造成敏感值尾部残留

- **记录时间**：2026-08-31
- **重现步骤**：执行 `redactSensitiveText(JSON.stringify({ apiToken: 'A"B', password: 'PASS' }))`。
- **预期结果**：输出中不得出现敏感值 `A"B` 的任何部分；敏感键值对应完整移除或替换为 `[REDACTED]`。
- **实际结果**：输出为类似 `{[REDACTED]B",[REDACTED]}`，`B` 及转义引号后的尾部仍残留。
- **影响**：包含转义引号的 token/key/secret/password/credential 值可部分进入 AI prompt，造成凭据泄漏。
- **建议**：对 JSON/字符串转义规则进行完整解析后再过滤，或使用能覆盖反斜杠转义字符的敏感赋值匹配；补充转义引号、反斜杠和换行值回归测试。
