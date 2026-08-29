# cr-p0-mcp — 进度

**轨 ID：** cr-p0-mcp  
**分支：** feature/cr-p0-mcp  
**状态：** 测试代理复验通过

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
| MCP-AUTH-1 | headless 无 `DATAZEN_MCP_TOKEN`（token 已存在） | exit 1 + stderr 提示配置 env | 【留待 R 回归】stdio 手工测 |
| MCP-AUTH-2 | headless 错误 `DATAZEN_MCP_TOKEN` | exit 1 + `invalid DATAZEN_MCP_TOKEN` | 【留待 R 回归】stdio 手工测 |
| MCP-AUTH-3 | headless 首次 bootstrap（无 `mcp.token`） | 创建 token 文件 + exit 1 提示 | 【留待 R 回归】stdio 手工测 |
| MCP-RES-1 | read_only + query-history | 列表隐藏；直接 read 返回 `[]` | 单测 `mcp_query_history_read_only_returns_empty` |
| MCP-RES-2 | allowlist + query-history | 仅 allowed connection 条目 | 单测 `mcp_query_history_respects_connection_allowlist` |
| MCP-RES-3 | allowlist + connections resource | 仅 allowed 连接 | 单测 `mcp_connections_resource_respects_allowlist` |
| MCP-RES-4 | allowlist + schema resource | disallowed connection 报错含 allowlist | 单测 `mcp_schema_resource_rejects_disallowed_connection` |

## 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib mcp --features "driver-postgres,driver-mysql,driver-sqlite,driver-redis"` | **106 passed; 0 failed** | 测试代理独立重跑（2026-08-29）；需先 `resolve-drivers --drivers=postgres,mysql,sqlite,redis` 注入 capabilities |

### 本轨新增单测（commit `808ee458`）

| 模块 | 测试 | 覆盖点 |
|------|------|--------|
| `auth.rs` | `load_or_create_token_persists_and_reloads` | token 持久化 |
| `auth.rs` | `verify_stdio_token_requires_env_after_bootstrap` | env 缺失/错误/正确 |
| `auth.rs` | `verify_stdio_token_rejects_first_run_without_env` | 首次 bootstrap exit 路径 |
| `permission.rs` | `read_only_hides_query_history_resource` | list 隐藏 + content gate |
| `server.rs` | `mcp_query_history_read_only_returns_empty` | read_only 直接 read 返回 `[]` |
| `server.rs` | `mcp_query_history_respects_connection_allowlist` | history allowlist 过滤 |
| `server.rs` | `mcp_connections_resource_respects_allowlist` | connections allowlist |
| `server.rs` | `mcp_schema_resource_rejects_disallowed_connection` | schema allowlist |

## 测试代理复验（2026-08-29）

| 检查项 | 结论 |
|--------|------|
| plan §cr-p0-mcp 五条验收 | 全部满足 |
| auth token（`mcp.token` + `DATAZEN_MCP_TOKEN`） | `run_mcp_stdio` 入口 gate；嵌入式 GUI 不走 token（文档化） |
| resource allowlist | connections / query-history / schema 均已过滤或拒绝 |
| read_only query-history | `list_resources` 隐藏；`read_resource` 返回 `[]` |
| `mcp.md` 文档 | 已更新 stdio 认证 + Resource/Tool 策略表 |
| Bug | 无 |

## 设计决策 / 遗留

- stdio 认证：`{appData}/mcp.token` + env `DATAZEN_MCP_TOKEN`；首次 bootstrap 创建 token 后要求配置 env 再启动
- 嵌入式 GUI duplex transport 不走 token（同进程信任边界）
- `read_only` 下 `query-history` 从 `list_resources` 隐藏，直接 read 返回空数组
- 遗留：Settings UI 未展示 token 路径；可考虑后续 IPC 暴露只读 token 位置
- Windows `mcp.token` 权限未设 ACL（`cr-p3-secrets-hardening` 轨范围）

## Commit

编码：`808ee45858e0beffd21dceea85a60f091e6347be`  
测试：`(pending)`
