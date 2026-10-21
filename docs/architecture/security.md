# 安全措施

> [返回架构总览](README.md)

## 1. 安全措施总览

| 安全措施 | 实现方式 | 位置 |
|----------|----------|------|
| **密码加密存储** | AES-256-GCM；主密钥在 OS 钥匙串或 `{appData}/.key`（见 `key_store`） | `Store::encrypt/decrypt` + `src-tauri/src/store/key_store.rs` |
| **密码派生** | Argon2id KDF（替代双轮 SHA-256），用于连接导入/导出文件与敏感字段的字段级加密 | `packages/feature-core/src/connection_import/datazen.rs::derive_argon2_key` |
| **AI Key 加密** | 随 Store 整体 AES-256-GCM 加密 | `src-tauri/src/store/ai_config.rs`（`ai_config.enc`） |
| **连接池管理** | sqlx 连接池 + 超时清理 | 各数据库驱动 |
| **空闲会话与资源回收** | Runtime 默认 30 分钟会话期限；桌面每 15 秒扫描，detached grace 为 60 秒；到期显式关闭而不透明重连 | `packages/runtime/src/application/idle.rs`、`src-tauri/src/platform/adapter.rs` |
| **连接生命周期** | 物理连接全部经 runtime 会话层租约借出（按用途授权、显式归还是常态路径），不是裸句柄长持有 | `packages/runtime/src/application/live.rs` |
| **结果集内存控制（按面生效）** | 分面限制：Workflow 单查询 `query_row_limit`（默认 1000 行）；AI 边界对 `rows`/`data` 等 payload 字段整体剔除（§4） | `packages/feature-core/src/workflow/executor.rs`、`packages/feature-core/src/ai/safety.rs` |
| **SQL 注入防护** | 参数化查询 | `query_with_params` |
| **TLS 加密后端** | 进程级 rustls `CryptoProvider` 显式安装（`aws-lc-rs`） | `src-tauri/src/tls.rs` |
| **CSP** | Content Security Policy | `src-tauri/tauri.conf.json` |
| **路径遍历防护** | 路径校验 | `src-tauri/src/commands/file.rs` |
| **SQL 安全（Safe Mode / 只读）** | 启发式语句分类 + 连接级拦截 | `packages/runtime/src/sql_guard`（宿主经 runtime 依赖调用） |

## 2. 加密存储

### 2.1 AES-256-GCM 加密

```
加密流程:
plaintext → 生成随机 nonce(12字节) → AES-256-GCM 加密 → base64(nonce || ciphertext)

解密流程:
encrypted → base64 解码 → 分离 nonce(前12字节) + ciphertext → 解密 → plaintext
```

加密主密钥由 `store/key_store.rs` 管理：**正式签名版默认存 OS 钥匙串**（macOS Keychain / Windows Credential Store / Linux Secret Service）；`DATAZEN_KEYRING=file` 或 macOS adhoc/未签名开发构建则使用 `{appData}/.key`。钥匙串不可用时可回退到已有 `.key`。首次启动随机生成 32 字节密钥。详见 [持久化存储 — 主加密密钥](backend/store.md#主加密密钥key_store)。

### 2.2 密码派生（导入/导出）

使用 Argon2id（`argon2` crate v0.5）进行密码派生，取代之前的双轮 SHA-256：

```rust
pub fn derive_argon2_key(password: &str, salt: &[u8]) -> Result<[u8; 32], FeatureError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| FeatureError::Internal(format!("Key derivation failed: {e}")))?;
    Ok(key)
}
```

## 3. Content Security Policy

CSP 以对象形式写在 `src-tauri/tauri.conf.json` 的 `security.csp`，当前生效值为：

```json
"csp": {
  "default-src": "'self'",
  "script-src": "'self' blob: asset: http://asset.localhost",
  "worker-src": "'self' blob:",
  "frame-src": "'self' datazen:",
  "style-src": "'self' 'unsafe-inline'",
  "img-src": "'self' data: asset: tauri: blob: datazen:",
  "font-src": "'self' blob: data:",
  "connect-src": "'self' ipc: http://ipc.localhost asset://localhost"
}
```

- `default-src 'self'` — 默认仅允许同源资源
- `script-src` 含 `blob: asset: http://asset.localhost` — Tauri 资源协议加载本地 bundle 所需，仍禁止远程脚本
- `connect-src 'self' ipc: http://ipc.localhost asset://localhost` — 仅 Tauri 2 的 IPC 通道，**不含任意外网**。AI Provider、MCP Server 等出站请求全部由 Rust 侧发起（`packages/ai-providers` 的 reqwest、`src-tauri/src/mcp/` 的 Rust client），webview 的 connect-src 不需要也不应放行 `https:` / `http:`
- `frame-src 'self' datazen:` — Workspace App 的 iframe 沙箱运行在 `datazen://` 协议下

## 4. AI 安全

| 关注点 | 策略 |
|--------|------|
| API Key 存储 | 加密存储在 `ai_config.enc`，不记录到日志 |
| 数据外发 | 普通数据库上下文默认仅发送 Schema 元数据（表名、列名、类型），不发送数据行；用户主动引用的本地文件、MCP 工具结果和自然语言输入仍属于用户明确选择的 prompt 内容，并在 Provider 边界再次清理 |
| 传输安全 | 所有 API 请求通过 HTTPS |
| 日志脱敏 | tracing 日志只保留元数据、长度、计数、状态和脱敏标记，不记录 API Key、prompt、SQL/error 正文、响应正文或 tool 参数 |
| Workflows 安全 | 路径遍历防护（ID 校验）、查询结果行数限制（1000） |

### 4.1 端到端数据边界

AI 请求经过两个边界：

1. **前端组装边界**：`src/lib/aiQueryActions.ts` 和 `src/stores/aiStore.ts` 在构造诊断上下文、发送 IPC 前，先清理 SQL、错误文本和 schema context。这个边界保证正常 UI 路径不会把明显的凭据直接传下去。
2. **Rust Provider 边界**：`packages/feature-core/src/ai/safety.rs` 的 `redact_for_gate` 是唯一公开脱敏入口；`src-tauri/src/ai/safety.rs` 只做兼容 re-export。每个 AI 用例在把 caller 内容交给 Provider 前都会再处理一次，直接调用 IPC、绕过 UI 或未来新增调用方都不能仅依赖前端脱敏。

最终发送给 Provider 的内容由具体 AI 场景决定，但允许的数据库上下文原则上只有：

- 数据库类型，以及有限的 database/schema/table/column/类型/约束元数据；
- 经清理、截断后的 SQL 和错误信息，用于诊断或 SQL 生成；
- 用户明确输入的自然语言和主动选择的本地上下文文件（文件路径本身也应避免包含机密）；
- AI Chat 中已连接 MCP Server 暴露的工具定义，以及工具调用后经过同一安全边界处理的结果。

以下内容禁止作为原始值进入 AI prompt：连接密码、AI API Key、Bearer/token、URI 中的用户名密码、连接 ID/会话 ID、完整查询结果/数据行、原始 MCP tool payload，以及被敏感字段标记的参数值。

`connectionContext` 当前采用显式字段允许列表：`name`、`host`、`port`、`serverVersion`、`readOnly`。这些字段不是凭据，但 host、连接名和版本仍可能属于敏感运维信息，调用方应避免使用真实客户名称或内部拓扑命名。

### 4.2 脱敏措施与示例

下面的“AI 结果”表示进入 Provider prompt 的值；`[REDACTED]` 是占位文本，敏感字段在结构化 JSON 中通常会被直接删除。

#### 1. 敏感字段按 key 删除

适用字段包括 `password`、`passwd`、`pwd`、`secret`、`token`、`authorization`、`bearer`、`credential`、`apiKey`、`privateKey` 等，匹配大小写和常见 camelCase/snake_case 变体。

```json
原始 schema/context:
{"table":"users","apiToken":"sk-live-123","columns":["id","email"]}

发送给 AI:
{"table":"users","columns":["id","email"]}
```

这样做的目的不是把 token 替换成一个“看似可用”的值，而是从结构化上下文中移除整个敏感字段。

#### 2. 敏感赋值、Bearer 和 URI 凭据替换

对无法解析为 JSON 的 SQL、错误文本或普通文本，使用模式匹配替换敏感值：

```text
原始错误:
connection failed: postgres://app_user:DbSecret@db.internal:5432/app
Authorization: Bearer eyJhbGciOi...
api_key=sk-live-123 password="DbSecret"

发送给 AI:
connection failed: postgres://[REDACTED]@db.internal:5432/app
Authorization: Bearer [REDACTED]
api_key=[REDACTED] password=[REDACTED]
```

URL query 中的 `token`、`access_token`、`api_key`、`secret`、`password` 等参数也会被替换。URI 的主机和端口可能仍会保留，因为它们有时用于诊断连接目标；如果主机名本身含客户信息，应由调用方避免送入或在输入前改为逻辑名称。

#### 3. 查询结果和 payload 整体剔除

结果数据不是“限制几行后就安全”。识别为 `data`、`rows`、`records`、`results`、`resultSet`、`queryResult`、`sampleRows`、`rawOutput`、`executionOutput`、`payload` 等结果/payload 字段时，整体不进入 AI 上下文：

```json
原始 tool/result:
{"columns":["id","email"],"rows":[{"id":7,"email":"alice@example.com"}],"rowCount":1}

发送给 AI:
{"columns":["id","email"],"rowCount":1}
```

因此 AI 可以知道列结构或行数等有限元数据，但不能通过这个边界读取客户行内容。需要分析结果时，应由产品先生成不含原始值的统计摘要，而不是把 `rows` 改名后继续发送。

#### 4. 递归清理嵌套对象，并限制上下文规模

清理不是只检查顶层字段；嵌套对象和数组会递归处理。为防止超大 schema、恶意嵌套或意外把整份文件送入模型，当前边界还限制：文本最多 4000 字节、JSON 深度最多 4、数组最多 100 项、对象最多 100 个 key。

```json
原始上下文:
{"debug":{"connection":{"password":"p@ss"}},"rows":[{"email":"alice@example.com"}],"tables":["users","orders"]}

发送给 AI:
{"debug":{"connection":{}},"tables":["users","orders"]}
```

超长文本按 UTF-8 安全边界截断，避免截断一个多字节字符造成无效文本；截断只控制暴露量，不会把本应删除的结果数据变成安全数据。

#### 5. Fix SQL 与 Retry 的本地边界

查询失败动作需要区分“给 AI 的诊断信息”和“本地编辑/执行所需的原文”：

```text
原始 SQL（仅保留在本地 QueryPanel）:
SELECT * FROM users WHERE email = 'alice@example.com' AND api_key = 'sk-live-123';

AI 诊断 prompt:
SELECT * FROM users WHERE email = 'alice@example.com' AND api_key = '[REDACTED]';

Fix SQL:
AI 只返回修复建议；原 SQL 由本地编辑器生成 draft，Apply to Editor 不自动执行。

Retry:
使用本地 SQL/绑定参数执行，但确认后重新校验 connection/session、SQL、参数和 context fingerprint。
```

这意味着本地编辑器可能继续显示原 SQL，但这不等于原 SQL 被发送给 AI 或写入日志。

### 4.3 日志安全示例

日志和 prompt 是两个不同的边界：即使某字段被允许进入诊断 prompt，也不应把正文复制到日志。日志只记录排障所需的低敏元数据：

```text
允许的日志:
ai_diagnose provider=openai model=gpt-4o sql_bytes=184 error_bytes=96 redacted=true
ai_tool_call name=query_db args_bytes=312 status=success

禁止的日志:
SELECT ... password='DbSecret' ...
{"rows":[{"email":"alice@example.com"}]}
{"api_key":"sk-live-123"}
完整 prompt、完整响应、MCP tool 参数和返回 payload
```

Provider 配置、AI Key 和自定义 endpoint 的日志只记录脱敏后的 provider/model/endpoint 元数据；endpoint 的 query secret、协议 payload 和 tool 参数正文都不记录。日志文件位于 `{data_dir}/logs/`，不能当作完整审计记录使用。

### 4.4 允许、禁止与仍需注意的内容

| 类别 | 可以暴露给 AI | 不可以暴露给 AI |
|------|---------------|-----------------|
| Schema | 表名、列名、类型、有限约束/索引元数据 | schema 描述中嵌入的密码、token、客户名单等秘密 |
| SQL/错误 | 清理并限长后的语句和错误文本 | 原始凭据、完整堆栈中的 secret、未清理的参数值 |
| 查询结果 | 列名、有限计数或由产品生成的安全统计摘要 | `rows`、`data`、`records`、完整 payload 和样例行 |
| 连接上下文 | allowlist 中的 name/host/port/version/readOnly | password、API Key、connectionId、dbSessionId |
| 本地文件/MCP | 用户主动选择且经边界清理的内容/工具定义 | 未经用户选择的文件、原始 tool args/result、凭据文件 |

当前实现是“字段/模式识别 + 边界限制”的纵深防御，并非形式化 DLP 或业务语义保密证明。任意自然语言、SQL 注释、错误文本或 schema 描述都可能包含模式无法识别的业务秘密；因此调用方仍不得主动把客户数据、生产凭据或机密业务规则写入 AI 输入。

## 5. SQL 安全（Safe Mode 与只读连接）

DataZen 在 GUI 查询执行、Driver Command、Workflow 与部分导出路径上，通过 `sql_guard::check_sql` 对 SQL 脚本做**尽力防护（best-effort heuristic）**——并非形式化安全证明，也无法覆盖所有方言、存储过程或绕过技巧。

### 5.1 只读连接（`readOnly`）

连接或数据库元数据标记为只读时，拦截常见写操作关键字（`INSERT` / `UPDATE` / `DELETE` / DDL / `GRANT` 等）。只读检查按分号拆分语句，对每条语句识别主动词；注释与引号内文本不参与分类。

**限制**：依赖关键字启发式，不能识别 `SELECT … INTO`、方言专有写语句或通过视图/函数间接写入等边界情况。

### 5.2 Safe Mode（全局设置，默认开启）

Safe Mode 额外约束：

| 规则 | 行为 |
|------|------|
| `UPDATE` / `DELETE` | 要求顶层 `WHERE` 子句（子查询内的 `WHERE` 不计） |
| `DROP` / `TRUNCATE` | 一律拦截 |

Safe Mode 开启时，Schema 树会隐藏 Truncate/Drop 等高危菜单项；关闭 Safe Mode 后，Query Panel 对 `DROP` / `TRUNCATE` 语句弹出二次确认，后端不再拦截此类语句。

**限制**：`WHERE 1=1` 等恒真条件可通过检查；多语句脚本中任一条违规即拒绝整批；与 MCP `SafeWrite` 权限模式（`mcp/permission.rs`）是不同边界，互不替代。

### 5.3 未覆盖路径

以下路径**不**经过 `sql_guard`，或仅部分检查：

- **Data Sync** 专用 IPC（`execute_data_sync`）——同族行级同步，语义与 Safe Mode 不同
- **Data Transfer** 破坏性模式——由传输向导独立确认
- **无头 MCP**（`--mcp-stdio`）——走 MCP 权限模式，非 GUI Safe Mode
- **驱动原生管理命令**（`admin_commands`）——按 Command 定义执行

### 5.4 用户预期

Safe Mode 与只读连接旨在降低误操作风险，**不能**替代数据库侧权限控制、审计或变更审批。生产环境应同时配置最小权限账号与外部治理流程。

## 6. 传输层与 rustls 加密后端

DataZen 的出站 TLS 统一走 `rustls 0.23`。该 crate 的 `CryptoProvider` 是**进程级单例**，只能由自身 Cargo feature 推断，而本项目的依赖图同时启用了两个 feature（`cargo tree -p datazen --edges normal,build -e features -i rustls@0.23.43`）：

| feature | 来源 |
|---------|------|
| `aws-lc-rs` | `reqwest`（feature `rustls`）、`tokio-rustls`（默认 feature） |
| `ring` | `tauri-plugin-updater`（feature `rustls-tls`）；启用 `redis` / `mongodb` / `sqlx` 等驱动时还会增加请求方 |

两个 feature 同时存在时，rustls 拒绝猜测：凡是**未显式指定 provider** 的 `ClientConfig::builder()` 都会 panic。因此 `src-tauri/src/tls.rs::install_default_crypto_provider()` 在 `main()` 的第一行显式安装 `aws-lc-rs`（rustls 0.23 自身的首选实现），并由各 TLS 构造点重复调用以覆盖不经 `main()` 的库/测试嵌入方。

`install_default()` 由 rustls 自身的 compare-and-swap 保证不会重复安装；DataZen 用 `OnceLock` 保证全进程只发起一次安装，**先到先得的竞态不会 panic**。需要注意的是 `tauri-plugin-updater` 会在没有默认 provider 时安装 `ring`，与本模块构成竞争——所以「安装得足够早」（`main()` 首行，早于 Tauri builder 初始化插件）是该修复成立的前提，而不仅仅是「幂等」。该竞争并不发生在插件注册阶段：`ring` 的安装语句位于 `tauri-plugin-updater-2.10.1/src/updater.rs:446-448` 的 `Updater::check()` 内，只有真正发起更新检查时才会执行；`main()` 优先是三重保障中最便宜的一重，即使任一依赖移动也仍然成立。

**输掉竞态的代价不止于隧道**。`reqwest` 并未显式 pin provider：`reqwest-0.13.4/src/async_impl/client.rs:719-721` **优先读取** `CryptoProvider::get_default()`，仅在进程无默认 provider 时才回退到自带的 `aws-lc-rs`。因此进程默认落在谁手里，所有 `reqwest` 客户端（全部 AI Provider 与基于 HTTP 的驱动）就跟谁走，切换时不会有任何提示。`sqlx`（`sqlx-core-0.8.6/src/net/tls/tls_rustls.rs:107`）与 `mongodb`（`mongodb-3.8.0/src/runtime/tls_rustls.rs:86`）才确实显式指定 provider，不受该选择影响。`rustls-platform-verifier` 需要分开看：`BuilderVerifierExt` 沿用调用方传入的 builder，而 `ConfigVerifierExt::with_platform_verifier()`（`rustls-platform-verifier-0.7.0/src/lib.rs:88`）调用的是**自动推断**的 `ClientConfig::builder()`——当前依赖图中没有调用方，所以不构成可达 panic，但不能笼统地把它记作「显式指定 provider」。
