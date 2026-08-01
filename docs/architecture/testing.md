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
