# B11：ai-providers 的 7 个 HTTP 出口绕过 NetworkProvider，SSRF 防护未覆盖

- **状态**：开放 · 已确认（只经静态检索，未做运行时 SSRF 探测）
- **归属**：本次下沉引入的**暴露面变化** —— 宿主的 provider 适配搬进 `packages/ai-providers` 后，
  仍各自持有 `reqwest::Client`，因此这些出口位于 `shared-boundaries-and-ports.md` §12 规定的 `NetworkProvider` 之外。
- **不阻断 Wave-4**：本轮裁定为「方案 1 —— 现在不改」，登记在案，另行排期。

## 症状

`docs/architecture/platform/team-server-and-auth.md` §12 规定出站 HTTP 必须经 `NetworkProvider`，
由它统一实施 SSRF 防护（拒绝私网 / 环回 / 链路本地 / 元数据地址）。
但 AI provider 侧不走这个端口，而是各自直接持有 `reqwest::Client`。

## 落点（`grep -rn 'use reqwest::Client' packages src-tauri`，实测 7 处）

| 文件 | 行 |
| --- | --- |
| `packages/ai-providers/src/custom.rs` | 9（`use reqwest::Client as HttpClient`） |
| `packages/ai-providers/src/protocol/mod.rs` | 13 |
| `packages/ai-providers/src/protocol/test_support.rs` | 4 |
| `packages/ai-providers/src/ollama.rs` | 5 |
| `packages/ai-providers/src/deepseek.rs` | 5 |
| `packages/ai-providers/src/anthropic.rs` | 5 |
| `packages/ai-providers/src/openai.rs` | 5 |

声明 `reqwest` 的 `Cargo.toml` 只有两个：`packages/ai-providers/Cargo.toml` 与 `src-tauri/Cargo.toml`。

## 为什么没在本波直接修

1. AI 端点由**用户自己配置**（含 custom 端点）。把它纳入 SSRF 白名单需要产品裁定：
   自建 Ollama / 局域网模型服务本身就是「私网地址」，一刀切拒绝会打断合法用法。
2. `NetworkProvider` 目前只有桌面适配器；Web / 服务端形态下的 AI 出口尚未定义
   —— 这正是共享层要解决的形态问题，不宜在迁移期顺手定型。
3. 因此本轮只登记、不编码。**修法待裁定**，不属于任何已授权轨道。

## 关闭条件

- 全仓不再出现绕过 `NetworkProvider` 的裸 `reqwest::Client` 出口；**或**
- 明确裁定「AI provider 出口豁免 SSRF 端口」，并把该豁免写进 `shared-boundaries-and-ports.md`
  （此时本缺陷按「已裁定豁免」关闭，而非按修复关闭）。

两种情况下结论都必须能从文档或代码读出来 —— 只写在本文件里不算关闭。
