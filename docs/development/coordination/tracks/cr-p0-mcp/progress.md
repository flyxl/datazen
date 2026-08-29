# cr-p0-mcp — 进度

**轨 ID：** cr-p0-mcp  
**分支：** feature/cr-p0-mcp  
**状态：** 编码完成

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
| MCP-AUTH-1 | headless 无 `DATAZEN_MCP_TOKEN` | exit 1 | 测试代理登记 |
| MCP-RES-1 | read_only + query-history | 返回 `[]` | 单测覆盖 |

## 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib mcp` | 106 passed | `--features driver-postgres,driver-mysql,driver-sqlite,driver-redis` |

## 设计决策 / 遗留

- stdio 认证：`{appData}/mcp.token` + env `DATAZEN_MCP_TOKEN`；首次 bootstrap 创建 token 后要求配置 env 再启动
- 嵌入式 GUI duplex transport 不走 token（同进程信任边界）
- `read_only` 下 `query-history` 从 `list_resources` 隐藏，直接 read 返回空数组
- 遗留：Settings UI 未展示 token 路径；可考虑后续 IPC 暴露只读 token 位置

## Commit

（提交后填写 hash）
