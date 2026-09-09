# JDBC Agent 实施方案

> **Status (2026-09-09):** MVP implemented on branch `feature/jdbc-agent` (Phases 0–4 core). Phase 5 items remain optional.

**Goal:** 交付可选的进程外 JDBC Agent + `JdbcDriver`，使 DataZen 在不嵌入 JVM、不破坏编译期驱动模型的前提下，连接仅有 JDBC 驱动的数据库。

**Architecture:** Host 侧 `JdbcDriver : DatabaseDriver` + `AgentProcessManager`；独立 Java 17+ Agent 子进程；stdio JSON-RPC 2.0；用户导入厂商 JAR；会话级 `DriverManager` 连接（MVP 无 Hikari，可后续加）。

**Tech Stack:** Tauri v2 / Rust (`packages/driver-api`, `packages/drivers/jdbc`) / Java 17+ Agent (`datazen-jdbc-agent/`) / JSON-RPC 2.0 over stdio / `DATAZEN_DRIVERS=…,jdbc`

**关联 PRD:** [jdbc-driver-support.md](./jdbc-driver-support.md)  
**用户文档:** [jdbc-guide.md](../features/jdbc-guide.md)

---

## 完成度

| Phase | 状态 |
|-------|------|
| 0 布局 + registry + hello | Done |
| 1 协议 + AgentProcessManager | Done |
| 2 Java session/query/meta/tx | Done（JDK-only） |
| 3 JdbcDriver + options 映射 | Done |
| 4 文档 / 稳定性基础 | Done（Settings UI / JAR 导入 UI 仍可增强） |
| 5 增强 | 未做（URL 推断逻辑类型、每连接独立 Agent 等） |

### 决策表

| 项 | 选择 |
|----|------|
| IPC | stdio + JSON-RPC 2.0 |
| 进程模型 | 单共享 Agent |
| Agent 位置 | **monorepo** `datazen-jdbc-agent/` |
| Registry id | `jdbc` |
| 连接配置 | `ConnectionConfig.options.jdbcUrl/jars/driverClass/props` |

### 构建

```bash
./datazen-jdbc-agent/build.sh
cargo test -p datazen-driver-jdbc
```

### 已知后续（非阻塞 MVP）

- [ ] Settings：JRE 路径、Agent jar 路径、空闲超时、JAR 库管理 UI
- [ ] 连接对话框专用 JDBC 字段（不依赖手写 options JSON）
- [ ] HikariCP 连接池（需 Gradle 依赖）
- [ ] `query_stream` 真流式 + `cancel_query_with_execution`
- [ ] i18n 文案与 e2e `DATAZEN_E2E_JDBC=1`
- [ ] 将本节架构摘要合并进 `docs/architecture/backend/drivers.md`

---

（历史任务清单见 git 历史；本文件以完成度表为准。）
