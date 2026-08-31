# v01x-object-actions Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-object-actions-BUG-001 | S3：对象搜索结果只返回匹配布尔值，不保留命中原因/字段；按 connection/database/schema/所属 table 命中时无法直接解释结果 | 待验证(修复后) | 2026-08-31；见下方 | 2026-08-31 独立 f1 发现；修复轮已为 `ObjectSearchResult` 增加 `matchedFields`/`matchReason`，覆盖 name/host/database/type/schema/object/table/column 命中及 column 所属 table 命中；定向回归 16 项通过，待独立测试代理复验 |

### v01x-object-actions-BUG-001 复现

1. 构造一个已加载索引，包含 `database: 'app'`、`schema: 'public'`、表 `users`、列 `email` 和函数 `refresh_users`。
2. 调用 `searchSchemaObjects(index, 'app')`，或调用 `searchSchemaObjects(index, 'users')` 观察列结果。
3. 检查返回的 `ObjectSearchResult`。

预期：结果包含可供 UI 展示的命中字段/原因（例如 `database` 命中、所属 `table` 命中），从而能解释对象为何出现在结果中。

实际（修复前）：`resultMatchesQuery` 仅返回 `boolean`；结果没有 `matchReason`、`matchedFields` 或等价字段。结果虽保留 connection/database/schema/table 上下文，但调用方无法区分具体命中来源。

修复后：非空查询的 `ObjectSearchResult` 保留所有命中字段和首要命中原因；column 结果可区分列名命中与所属 table 命中，空搜索保持空命中元数据。

影响量级：按 database、schema、connectionName 或 column 所属 table 搜索时，搜索结果无法直接解释命中原因；只能由调用方重复实现匹配逻辑。

## 基线验证遗留（非本轨功能 bug）

- 目标 worktree 没有被 git 跟踪的 `src/locales/builtinLocales.ts`；它被现有 `src/locales/index.ts` 引用，导致 tsc/未 mock 的模块测试无法完整加载。该文件是 ignored 生成文件，本轨不生成、不提交。
- 现有 `src/windows/settings/SettingsContent.tsx:66` 仍有隐式 `any` 诊断；本轨不修改共享设置页面。
- Host 桌面 E2E 依 playbook 登记留待 R；本轨不接共享页面，也不直接执行 SQL。
