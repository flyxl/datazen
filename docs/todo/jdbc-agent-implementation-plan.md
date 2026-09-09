# JDBC Agent 实施方案

> **Status (2026-09-09):** MVP + short-term UX + streaming path on `feature/jdbc-agent`.

**Goal:** 可选进程外 JDBC Agent + `JdbcDriver`，不嵌入 JVM。

**Architecture:** Host `JdbcDriver` + `AgentProcessManager`；Java 17+ Agent；stdio JSON-RPC 2.0；用户自备厂商 JAR。

**关联:** [jdbc-driver-support.md](./jdbc-driver-support.md) · [jdbc-guide.md](../features/jdbc-guide.md) · [resolve-drivers materialize](./jdbc-resolve-drivers-materialize.md)

---

## 完成度

| Phase | 状态 |
|-------|------|
| 0–3 布局 / Agent / JdbcDriver | Done |
| 4 Settings Host 同步 (pluginSettings.jdbc) | Done |
| 短期：连接表单 jar 多选 / URL 推断 driverClass | Done |
| 短期：Settings PathInput + 错误文案 | Done |
| 中期：query_stream 分批 + cancel 钩子 | Done（非精确 executionId 取消） |
| 中期：e2e 开关 `DATAZEN_E2E_JDBC=1` | Done（骨架） |
| HikariCP | 未做（仍 JDK DriverManager 会话连接；可后续 Gradle） |
| resolve-drivers.mjs 固化 | 见 materialize TODO |

### 决策

| 项 | 选择 |
|----|------|
| IPC | stdio JSON-RPC 2.0 |
| 进程 | 单共享 Agent + settings epoch 重启 |
| Settings | `pluginSettings.jdbc` ↔ `apply_plugin_settings` |
| 流式 | `query.execute` / `query.fetch` 批次 → `QueryStreamEvent` |

### 构建

```bash
./datazen-jdbc-agent/build.sh
DATAZEN_DRIVERS=basic,jdbc cargo test -p datazen-driver-jdbc
```
