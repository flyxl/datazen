# 测试策略

> [返回架构总览](README.md)

## 1. 测试分层

| 层级 | 工具 | 范围 |
|------|------|------|
| Rust 单元测试 | `cargo test` | 各模块 `#[cfg(test)]` |
| Rust 集成测试 | `cargo test` | `src-tauri/tests/` |
| 前端单元测试 | Vitest | `src/**/__tests__/` |
| E2E 测试 | WebdriverIO | `e2e/specs/`（35 spec） |
| 手工黑盒测试 | computer-use-mcp | `test/` |

## 2. 运行命令

```bash
# Rust 测试
cargo test -p datazen              # 主应用单元测试
cargo test -p datazen-ai-api       # AI API 单元测试
cargo test -p datazen-driver-api   # Driver API 单元测试

# 前端测试
pnpm test:unit                     # package.json 脚本（vitest run）
npx vitest run                     # 同上
npx vitest run --reporter=verbose  # 详细输出

# E2E 测试（详见 docs/e2e-testing.md — 必须 Tauri + webdriver 构建）
pnpm e2e                           # 完整构建 + 全部 E2E（推荐）
pnpm e2e:skip-build                # 仅当已有合格 webdriver 二进制
pnpm e2e:core / e2e:db / e2e:ai    # 分组（默认 skip-build）
```

> ⚠️ **禁止** `cargo build --features webdriver` 作为 E2E 二进制来源。  
> 正确命令：`pnpm tauri build --debug --features webdriver`。完整说明：[e2e-testing.md](../e2e-testing.md)。

## 3. Rust 测试覆盖

### 3.1 单元测试

各模块内 `#[cfg(test)]` 模块：

| 模块 | 测试范围 |
|------|---------|
| `packages/ai-api/tests/api_tests.rs` | AiProvider trait、types、factory（17 tests） |
| `src-tauri/src/ai/registry.rs` | AiProviderRegistry（7 tests） |
| `src-tauri/src/ai/openai.rs` | OpenAI Provider（7 tests） |
| `src-tauri/src/ai/anthropic.rs` | Anthropic Provider（5 tests） |
| `src-tauri/src/ai/prompt_resolver.rs` | PromptResolver 解析优先级、多语言 fallback |
| `src-tauri/src/mcp/server.rs` | MCP Server tools（6 tests） |
| `src-tauri/src/workflow/workflows.rs` | Workflows 系统 |
| `src-tauri/src/mcp/client.rs` | MCP Client（3 tests） |
| `src-tauri/src/commands/context.rs` | AI 上下文文件（路径遍历防护、扩展名白名单、大小限制，14 tests） |

### 3.2 集成测试

| 文件 | 内容 |
|------|------|
| `src-tauri/tests/ai_e2e.rs` | AI 功能端到端测试（NL2SQL、诊断、EXPLAIN、Chat 等）。需真实 LLM API Key，配置 `.env.test` |
| `src-tauri/tests/workflow_tests.rs` | 跨数据库 Workflow 测试。需本地 PostgreSQL + MySQL，配置 `.env` |

## 4. 前端测试覆盖

### 4.1 组件测试

| 测试文件 | 覆盖范围 |
|---------|---------|
| `components/DataTable/__tests__/` | CellRenderer、EditableCell、TableHeader、VirtualBody（5 文件） |
| `components/ai/__tests__/AiInput.test.tsx` | AiInput 渲染、@ 触发、context chips、发送/停止按钮（10 tests） |
| `components/ai/__tests__/ContextPicker.test.tsx` | 加载状态、渲染、搜索过滤、选择、键盘导航（10 tests） |
| `components/connection/__tests__/useConnectionForm.test.ts` | 连接表单验证 |
| `components/ui/__tests__/Button.test.tsx` | Button 组件 |

### 4.2 Store 测试

| 测试文件 | 覆盖范围 |
|---------|---------|
| `stores/__tests__/queryStore.test.ts` | 查询 Store |
| `stores/__tests__/schemaStore.test.ts` | Schema Store |
| `stores/__tests__/tableDataStore.test.ts` | 表数据 Store |

### 4.3 工具库测试

| 测试文件 | 覆盖范围 |
|---------|---------|
| `lib/__tests__/databaseTypes.test.ts` | DB_REGISTRY 元数据、能力 opt-in 标志 |
| `locales/locales.test.ts` | 10 语系 key parity、Beta 语系 English 占位检测 |
| `lib/__tests__/formatters.test.ts` | 数据格式化工具 |
| `lib/__tests__/rowToRecord.test.ts` | 行数据转换 |
| `lib/__tests__/extractSql.test.ts` | SQL 提取 |
| `lib/__tests__/extractQuestions.test.ts` | AI 追问解析 |
| `lib/__tests__/aiProviders.test.ts` | AI Provider 工具 |
| `lib/sqlDialects/__tests__/dialects.test.ts` | SQL 方言策略 |
| `lib/chart/__tests__/` | 图表引擎（fieldInference、recommend、transform、nlConfig — 4 文件） |

### 4.4 ER 图测试

| 测试文件 | 覆盖范围 |
|---------|---------|
| `windows/connection/er/__tests__/buildErGraph.test.ts` | ER 图 graph 构建（31 tests）：节点/边创建、FK 列标记、焦点模式、布局、自引用 FK、大数据集、高亮/暗化、复合 FK |

### 4.5 Hook 测试

| 测试文件 | 覆盖范围 |
|---------|---------|
| `hooks/__tests__/computeColumnWidths.test.ts` | 列宽计算 |

## 5. E2E 测试

> **Agent / 开发者操作手册（构建、排错、检查清单）：[docs/e2e-testing.md](../e2e-testing.md)**

WebdriverIO E2E spec（Host：`e2e/specs/`）：

| 领域 | Spec 文件 |
|------|----------|
| **核心 UI** | `main-window.ts`, `homepage-features.ts`, `settings.ts`, `i18n-menu.ts`, `drag-drop-groups.ts`, `detail-panel.ts`, `file-connection-fields-theme.ts` |
| **连接** | `new-connection.ts`, `edit-delete-connection.ts`, `connection-search-group.ts`, `connection-window.ts` |
| **SQL / 数据** | `sql-query.ts`, `table-data.ts`, `table-edit.ts`, `table-structure.ts`, `data-types.ts`, `export-import.ts`, `er-diagram.ts`, `chart-expand.ts` |
| **数据库驱动（Host）** | `sqlite.ts`, `mysql.ts` |
| **AI / Workflow** | `ai-features.ts`, `ai-ask-question.ts`, `ai-context.ts`, `workflow.ts`, `workflow-window.ts` |
| **路径 IPC / 备份·i18n** | `path-ipc-hardening.ts`, `app-data-backup.ts`, `i18n-10-locales.ts`, `system-locale.ts` |
| **运维** | `backup-database.ts`, `data-sync-real.ts`, `bugfix-verification.ts` |

**插件自有（不进 Host 默认 `pnpm e2e` / `pnpm test:unit`）：**

| 类型 | 位置 / 命令 |
|------|-------------|
| Redis UI 单测 | `packages/drivers/redis/ui/__tests__/` — `pnpm test:unit:drivers` |
| Redis E2E | `packages/drivers/redis/e2e/` — `pnpm e2e:redis` |
| Kiwi 元数据单测 | `datazen-driver-kiwi` `ui/plugin-meta.test.ts` |
| Kiwi E2E | `datazen-driver-kiwi`：`pnpm e2e:kiwi` |

配置文件：`e2e/.env`（`e2e/.env.example` 示例）。跑库相关 spec 前执行 `bash e2e/setup-e2e-env.sh`（`e2e/run.mjs` 也会调用）。

**构建要求：** `pnpm tauri build --debug --features webdriver`（禁止裸 `cargo build`）。

快捷运行：
```bash
pnpm e2e                # 完整构建 + Host 全部
pnpm e2e:skip-build     # 已有合格二进制时
pnpm e2e:core           # 核心 UI
pnpm e2e:db             # 数据库驱动（Host）
pnpm e2e:redis          # Redis 深度（显式）
# Kiwi：cd datazen-driver-kiwi && pnpm e2e:kiwi
pnpm e2e:ai             # AI 功能
pnpm e2e:i18n-backup    # 备份 + 10 语言
pnpm e2e:path-ipc       # 路径 IPC 加固
pnpm test:unit:drivers  # Path 驱动 UI 单测
```

## 6. 手工黑盒测试

`test/` 目录下维护手工黑盒测试文档，使用 `computer-use-mcp` 桌面自动化辅助执行：

```
test/
├── test-plan.md            # 测试计划（范围、策略、环境）
├── test-cases.md           # 测试用例（102 个，覆盖 14 个模块）
├── test-results.md         # 测试执行结果与汇总
├── bug-list.md             # Bug 清单索引
├── mcp-setup.md            # computer-use-mcp 配置说明
├── bugs/                   # 每个 Bug 一个文件
│   ├── BUG-001.md ~ BUG-008.md
└── screenshots/            # Bug 截图证据
```

### 6.1 测试模块覆盖

| 模块 | 用例数 | 说明 |
|------|--------|------|
| 连接管理 | 18 | CRUD、类型切换、表单验证、分组搜索 |
| 数据库连接窗口 | 6 | Schema 浏览、导航、数据库切换 |
| 表数据浏览 | 10 | 分页、排序、筛选、行内编辑 |
| SQL 查询 | 10 | 执行、取消、历史、收藏、多标签 |
| 表结构 | 6 | 列信息、索引、约束查看 |
| AI 功能 | 9 | NL2SQL、诊断、EXPLAIN、Chat、NL 筛选 |
| 数据同步 | 4 | 配置、比较、执行、断点续传 |
| 备份恢复 | 4 | 备份/恢复配置与执行 |
| 设置 | 9 | 主题、语言、AI 配置、快捷键 |
| 其他 | 26 | Redis、导出、多窗口、快捷键、边界容错 |

### 6.2 Bug 报告格式

每个 Bug 文件包含：基本信息、环境配置、重现步骤、预期/实际结果、根因分析、修复方案、回归验证记录。
