# BUG · ai-providers 手写了第二份更弱的日志脱敏

**状态**：开放 · Wave-3 必修 · **C3 轨不可修**（该轨基线不含 A2 代码）

## 症状

`packages/ai-providers` 有一套自己的端点脱敏，与 `platform-infra::redaction` 功能重叠但更弱。

## 位置

- 实现 `packages/ai-providers/src/protocol/mod.rs:85-104` `redact_endpoint_for_log`
- 测试 `:356` `redact_endpoint_for_log_masks_userinfo_credentials`、`:368` `…leaves_plain_endpoints_intact`
- 正确的实现 `packages/platform-infra/src/redaction.rs`（C3 轨交付）

## 弱在哪

| 能力 | platform-infra | ai-providers 版 |
| --- | --- | --- |
| 去 query | 逐项脱敏 | `split_once('?')` 整段丢弃 |
| 去 userinfo | 正则替换 | `split_once('@')` 掩码 |
| `Bearer` 头 | 有 | **无** |
| JSON password 字段 | 有 | **无** |

零 `Regex::new`、零 `OnceLock` —— 是手写的字符串切割。

## 影响

A2 迁移时为图快在新 crate 里就地重写，没有复用宿主已有的 `log_redact`。
**这是本轮 Wave-2 制造出的唯一一处实现质量退化**，也是「共享基础设施要有归处」的直接证据。

## 修法

删除 `protocol/mod.rs:85-104` 与两个测试，改为调用 `datazen_platform_infra::redaction::redact_url_for_log`；
`packages/ai-providers/Cargo.toml` 加 `datazen-platform-infra` 依赖。

## 关闭条件

`cargo test -p datazen-ai-providers` 通过，且全仓 `redact_endpoint_for_log` 为 0 命中。

---

来源：Wave-2 合并后于集成分支 `d268a7ded` 实测。`packages/platform-infra/src/redaction.rs` 的模块注释已记录该重复的来由。
