# v01x-filter-pagination Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| `v01x-filter-pagination-BUG-001` | **S3**：`FilterEditor` busy 时，已完成条件 chip 的删除入口仍可触发 `onRemove`，loading/busy 禁止过滤变更契约不完整 | 待验证 | 见下方详细步骤 | 2026-08-31 独立临时 RTL 探针 1/1 复现；修复后待测试代理复验 |

## 环境记录

- worktree 缺少 gitignore 的 `src/locales/builtinLocales.ts`；该目录由沙箱拒绝写入。组件测试通过临时 resolver 使用外部只读 stub 验证，未修改 locale/codegen，也未将环境问题登记为业务 bug。
- 本轨无 Rust 改动；Host 桌面 E2E 按约定留待 R。

## v01x-filter-pagination-BUG-001

- 严重等级：S3
- 状态：待验证
- 发现时间：2026-08-31 08:17（Asia/Shanghai）
- 关联文件：`src/components/FilterEditor.tsx` 的 `FilterConditionChip`（约 164-196 行）

### 重现步骤

1. 渲染 `FilterEditor`，传入一个完整条件，例如 `name = 'alice'`，并设置 `loading={true}`。
2. 保持编辑器展开，点击完成条件 chip 的删除按钮（`filter.remove`）。
3. 观察 caller 的 `onRemove` 回调。

### 预期结果

loading/busy 期间所有过滤条件变更入口均不可操作，`onRemove` 不应被调用。

### 实际结果

完成条件以 `FilterConditionChip` 渲染，其删除按钮没有 `disabled={loading}`；独立探针实测 `onRemove(0)` 被调用一次。展开后的 Select、Value、Apply 等控件虽被禁用，但 chip 删除绕过了 busy 保护。

### 验证记录

| 日期 | 验证人 | 方法 | 结果 |
|---|---|---|---|
| 2026-08-31 | 独立测试子代理 | 临时 Vitest + React Testing Library，`loading=true` 点击完成条件 chip 删除 | 复现；待修复代理处理后重新验证 |
