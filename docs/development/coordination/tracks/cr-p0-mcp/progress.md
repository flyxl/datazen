# cr-p0-mcp — 进度

**轨 ID：** cr-p0-mcp  
**分支：** feature/cr-p0-mcp  
**状态：** 已完成

## 范围

MCP stdio 本地认证；Resource 读取应用 permission mode + connection allowlist；`query-history` 受 allowlist 约束。

## 验收

- [x] 未授权 MCP client 无法使用 stdio 服务（`auth.rs` 单测 + `mcp.md` E2E 步骤）
- [x] query-history 受 allowlist；read_only 不泄露 SQL（返回 `[]`）
- [x] connections / schema resources 与 tool allowlist 一致；workflows 与 `list_workflows` 一致
- [x] `cargo test -p datazen --lib mcp` 全绿（106 passed）

## E2E 用例

| ID | 场景 | 断言 | 执行时机 |
|----|------|------|----------|
| MCP-AUTH-1 | headless 无 `DATAZEN_MCP_TOKEN`（token 已存在） | exit 1 + stderr 提示配置 env | `mcp_stdio_rejects_missing_env` |
| MCP-AUTH-2 | headless 错误 `DATAZEN_MCP_TOKEN` | exit 1 + `invalid DATAZEN_MCP_TOKEN` | `mcp_stdio_rejects_invalid_token` |
| MCP-AUTH-3 | headless 首次 bootstrap（无 `mcp.token`） | 创建 token 文件 + exit 1 提示 | `mcp_stdio_bootstrap_creates_token_and_exits` |
| MCP-AUTH-4 | headless 正确 token | 进程通过认证并启动 MCP server | `mcp_stdio_accepts_valid_token_and_starts_server` |
| MCP-RES-1 | read_only + query-history | 列表隐藏；直接 read 返回 `[]` | 单测 `mcp_query_history_read_only_returns_empty` |
| MCP-RES-2 | allowlist + query-history | 仅 allowed connection 条目 | 单测 `mcp_query_history_respects_connection_allowlist` |
| MCP-RES-3 | allowlist + connections resource | 仅 allowed 连接 | 单测 `mcp_connections_resource_respects_allowlist` |
| MCP-RES-4 | allowlist + schema resource | disallowed connection 报错含 allowlist | 单测 `mcp_schema_resource_rejects_disallowed_connection` |

## 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib mcp --features "driver-postgres,driver-mysql,driver-sqlite,driver-redis"` | **106 passed; 0 failed** | 测试代理独立重跑（2026-08-29） |
| `CARGO_TARGET_DIR=target cargo test -p datazen --test mcp_stdio_auth --features "driver-postgres,driver-mysql,driver-sqlite,driver-redis"` | **4 passed; 0 failed** | stdio token 子进程集成测（2026-08-29） |

## 设计决策 / 遗留

- stdio 认证：`{appData}/mcp.token` + env `DATAZEN_MCP_TOKEN`
- 嵌入式 GUI duplex transport 不走 token（同进程信任边界）
- Windows `mcp.token` ACL 留待 `cr-p3-secrets-hardening`

## Commit

编码：`808ee458` | 测试：`1e672b81`
