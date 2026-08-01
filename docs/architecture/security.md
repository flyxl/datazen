# 安全措施

> [返回架构总览](README.md)

## 1. 安全措施总览

| 安全措施 | 实现方式 | 位置 |
|----------|----------|------|
| **密码加密存储** | AES-256-GCM + 系统密钥链 | `Store::encrypt/decrypt` |
| **密码派生** | Argon2id KDF（替代双轮 SHA-256） | `commands/config.rs::derive_key_from_password` |
| **AI Key 加密** | 随 Store 整体 AES-256-GCM 加密 | `store/mod.rs::ai_config.enc` |
| **连接池管理** | sqlx 连接池 + 超时清理 | 各数据库驱动 |
| **空闲连接清理** | 定时任务（每 5 分钟） | `ConnectionManager::start_cleanup_task` |
| **连接泄露检测** | 守卫模式 + 超时警告 | `ConnectionGuard` |
| **内存限制** | 结果集大小检查 | `QueryResultLimiter` |
| **SQL 注入防护** | 参数化查询 | `query_with_params` |
| **CSP** | Content Security Policy | `tauri.conf.json` |
| **路径遍历防护** | 路径校验 | `commands/file.rs` |

## 2. 加密存储

### 2.1 AES-256-GCM 加密

```
加密流程:
plaintext → 生成随机 nonce(12字节) → AES-256-GCM 加密 → base64(nonce || ciphertext)

解密流程:
encrypted → base64 解码 → 分离 nonce(前12字节) + ciphertext → 解密 → plaintext
```

加密密钥从系统密钥链（macOS Keychain / Windows Credential Store / Linux Secret Service）获取，首次启动时随机生成。

### 2.2 密码派生（导入/导出）

使用 Argon2id（`argon2` crate v0.5）进行密码派生，取代之前的双轮 SHA-256：

```rust
fn derive_key_from_password(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Key derivation failed: {e}"))?;
    Ok(key)
}
```

## 3. Content Security Policy

```json
"csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: asset: tauri:; connect-src 'self' https: http:"
```

- `default-src 'self'` — 默认仅允许同源资源
- `connect-src 'self' https: http:` — 允许连接外部 API（AI Provider、MCP Server）
- `script-src 'self'` — 禁止内联脚本和外部脚本

## 4. AI 安全

| 关注点 | 策略 |
|--------|------|
| API Key 存储 | 加密存储在 `ai_config.enc`，不记录到日志 |
| 数据外发 | 默认仅发送 Schema 元数据（表名、列名、类型），不发送数据行 |
| 传输安全 | 所有 API 请求通过 HTTPS |
| 日志脱敏 | tracing 日志中不记录 API Key |
| Skills 安全 | 路径遍历防护（ID 校验）、查询结果行数限制（1000） |
