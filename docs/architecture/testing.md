# 测试策略

> [返回架构总览](README.md)

## 1. 测试分层

| 层级 | 工具 | 范围 |
|------|------|------|
| Rust 单元测试 | `cargo test` | 各模块 `#[cfg(test)]` |
| Rust 集成测试 | `cargo test` | `src-tauri/tests/` |
| 前端单元测试 | Vitest | `src/**/__tests__/` |
| E2E 测试 | WebdriverIO | `e2e/specs/` |

## 2. 运行命令

```bash
# Rust 测试
cargo test -p datazen              # 主应用单元测试
cargo test -p datazen-ai-api       # AI API 单元测试
cargo test -p datazen-driver-api   # Driver API 单元测试

# 前端测试
npx vitest run                     # 所有前端单元测试
npx vitest run --reporter=verbose  # 详细输出

# E2E 测试
pnpm e2e                           # WebdriverIO E2E
```

## 3. Rust 测试覆盖

### 3.1 单元测试

各模块内 `#[cfg(test)]` 模块：

| 模块 | 测试范围 |
|------|---------|
| `packages/ai-api/tests/api_tests.rs` | AiProvider trait、types、factory（17 tests） |
| `src-tauri/src/ai/registry.rs` | AiProviderRegistry（7 tests） |
| `src-tauri/src/ai/openai.rs` | OpenAI Provider（7 tests） |
| `src-tauri/src/ai/anthropic.rs` | Anthropic Provider（5 tests） |
| `src-tauri/src/ai/prompt.rs` | Prompt 模板 |
| `src-tauri/src/mcp/server.rs` | MCP Server tools（6 tests） |
| `src-tauri/src/mcp/skills.rs` | Skills 系统（9 tests） |
| `src-tauri/src/mcp/client.rs` | MCP Client（3 tests） |

### 3.2 集成测试

`src-tauri/tests/ai_e2e.rs` — AI 功能端到端测试（需真实 LLM API Key）：
- 配置文件: `.env.test`（gitignored）
- 覆盖: NL2SQL、错误诊断、EXPLAIN 分析、Chat、Schema 文档、连接诊断、查询分析

## 4. 前端测试覆盖

| 测试文件 | 覆盖范围 |
|---------|---------|
| `components/DataTable/__tests__/` | CellRenderer、EditableCell、TableHeader、VirtualBody |
| `lib/__tests__/databaseTypes.test.ts` | DB_REGISTRY 元数据验证 |
| `lib/__tests__/formatters.test.ts` | 数据格式化工具 |
| `lib/__tests__/rowToRecord.test.ts` | 行数据转换 |
| `lib/sqlDialects/__tests__/dialects.test.ts` | SQL 方言策略 |
| `stores/__tests__/queryStore.test.ts` | 查询 Store |
| `stores/__tests__/schemaStore.test.ts` | Schema Store |
| `stores/__tests__/tableDataStore.test.ts` | 表数据 Store |
| `hooks/__tests__/computeColumnWidths.test.ts` | 列宽计算 |
| `components/connection/__tests__/useConnectionForm.test.ts` | 连接表单 Hook |

## 5. E2E 测试

`e2e/specs/ai-features.ts` — AI 功能 WebdriverIO E2E 测试：
- 配置文件: `e2e/.env.example`
- 覆盖: AI 设置、NL2SQL、AI Chat、智能筛选

## 6. 手工黑盒测试

`test/` 目录下维护手工黑盒测试文档，使用 `computer-use-mcp` 桌面自动化辅助执行：

```
test/
├── test-plan.md            # 测试计划（范围、策略、环境）
├── test-cases.md           # 测试用例（102 个，覆盖 14 个模块）
├── test-results.md         # 测试执行结果与汇总
├── bug-list.md             # Bug 清单索引
├── bugs/                   # 每个 Bug 一个文件
│   ├── BUG-001.md ~ BUG-007.md
└── mcp-setup.md            # computer-use-mcp 配置说明
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
