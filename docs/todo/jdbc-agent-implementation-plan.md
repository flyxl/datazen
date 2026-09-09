# JDBC Agent 实施方案

> **For agentic workers:** 实现本计划时 REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`。任务使用 checkbox（`- [ ]`）跟踪进度。

**Goal:** 交付可选的进程外 JDBC Agent + `JdbcDriver`，使 DataZen 在不嵌入 JVM、不破坏编译期驱动模型的前提下，连接仅有 JDBC 驱动的数据库。

**Architecture:** Host 侧 `JdbcDriver : DatabaseDriver` + `AgentProcessManager`；独立 Java 17+ Agent 子进程；stdio JSON-RPC 2.0；用户导入厂商 JAR；HikariCP + 分批 fetch。

**Tech Stack:** Tauri v2 / Rust (`packages/driver-api`, `packages/drivers/jdbc`) / Java 17+ Agent fat-jar / JSON-RPC 2.0 over stdio / 可选 `DATAZEN_DRIVERS=…,jdbc`

**文档日期:** 2026-09-09  
**关联 PRD:** [jdbc-driver-support.md](./jdbc-driver-support.md)

---

## Global Constraints

- 主程序默认**不**依赖 JRE；Basic SKU 与默认构建不含 JDBC 路径
- **禁止**进程内嵌 JVM / JNI / 运行时动态加载 Rust `.so` 驱动
- 上层只走现有 `DatabaseDriver`；不新增平行连接/查询 Host API
- Host 测试：`src-tauri/`、`src/**/__tests__/`、`e2e/specs/`；Agent / JDBC 专属测试放 Agent 工程或 `packages/drivers/jdbc/`
- 厂商 JDBC JAR **不**随 DataZen 分发；测试夹具优先 H2（许可证清晰）或 wiremock 级 mock Agent
- i18n 开发期仅改 `src/locales/en.ts` 与可选 `zh-CN.ts`
- 错误统一 `DriverError` / `CommandError`；日志与错误消息 redact 密码与敏感 URL 段
- PR 合并前：相关 crate 的 `cargo test` + 涉及 UI 时 `pnpm test:unit`；启用 jdbc 的 job 再跑 Agent 集成测

---

## 0. 仓库与模块布局（先锁定）

### 0.1 建议目录

```text
packages/drivers/jdbc/              # Rust path driver（JdbcDriver + 协议客户端）
  Cargo.toml
  src/lib.rs
  src/driver.rs                     # DatabaseDriver impl
  src/agent_process.rs              # spawn / 读写 / 生命周期
  src/protocol.rs                   # JSON-RPC 编解码与 method 常量
  src/types.rs                      # session / cursor / value 映射
  protocol.md                       # 契约文档（与 Java 共享语义）
  ui/meta.ts                        # connectionView, 表单字段

datazen-jdbc-agent/                 # 独立 Java 工程（可 monorepo 子目录或独立 repo）
  build.gradle.kts / pom.xml
  src/main/java/.../AgentMain.java
  src/main/java/.../JsonRpcLoop.java
  src/main/java/.../SessionManager.java
  src/main/java/.../JdbcExecutor.java
  src/main/java/.../MetaService.java
  src/main/java/.../TypeCodec.java
  src/test/java/...

src-tauri/src/db/                   # 若需 Host 侧薄封装，优先仍放在 driver crate
docs/todo/jdbc-driver-support.md    # PRD（已有）
docs/todo/jdbc-agent-implementation-plan.md  # 本文件
```

**决策（本计划默认）：**

| 项 | 选择 |
|----|------|
| IPC | stdio + JSON-RPC 2.0（一行一个 JSON） |
| 进程模型 | **单共享 Agent**（全应用一个 JVM） |
| Agent 分发 | 独立组件 / 可选下载；**不**打进 Basic 安装包 |
| Registry id | `jdbc`，`source: path`，`feature: driver-jdbc` |
| 逻辑库类型 | MVP 固定 `jdbc`（展示名 Generic JDBC） |

- [ ] **Task 0.1:** 确认 Agent 放 monorepo 子目录还是独立 Git repo；更新本文件决策表
- [ ] **Task 0.2:** 在 `drivers-registry.json` 增加 `jdbc` 条目（可先 `description` 标明 WIP）
- [ ] **Task 0.3:** 创建 `packages/drivers/jdbc` crate 骨架 + `inventory` 注册（空实现返回 Unsupported）
- [ ] **Task 0.4:** 创建 `datazen-jdbc-agent` 空工程：读一行 JSON、回 `agent.hello`

**验收：** `DATAZEN_DRIVERS=basic,jdbc` 能编译；拉起 mock/真 Agent 完成 hello 往返。

---

## Phase 1：协议与 AgentProcessManager

### 1.1 协议契约（`protocol.md`）

锁定以下 method 与最小字段（实现时可加可选字段，不可擅自改语义）：

| Method | Params（核心） | Result（核心） |
|--------|----------------|----------------|
| `agent.hello` | `hostVersion`, `protocolVersion` | `agentVersion`, `protocolVersion`, `capabilities` |
| `agent.shutdown` | — | `ok` |
| `session.open` | `url`, `user`, `password`, `driverClass?`, `jars[]`, `props{}` | `sessionId` |
| `session.close` | `sessionId` | `ok` |
| `meta.databases` | `sessionId` | `{ name }[]` |
| `meta.tables` | `sessionId`, `database?`, `schema?` | `{ name, type }[]` |
| `meta.columns` | `sessionId`, `database?`, `schema?`, `table` | 列描述数组 |
| `query.execute` | `sessionId`, `sql`, `maxRows`, `fetchSize?` | `columns`, `rows`, `cursorId?`, `hasMore`, `executionId?` |
| `query.fetch` | `sessionId`, `cursorId`, `maxRows` | `rows`, `hasMore` |
| `query.close` | `sessionId`, `cursorId` | `ok` |
| `query.cancel` | `sessionId`, `executionId` | `ok` |
| `exec.update` | `sessionId`, `sql` | `updateCount` |
| `tx.begin` / `tx.commit` / `tx.rollback` | `sessionId` | `ok` |

错误对象：

```json
{
  "code": -32001,
  "message": "human readable, no password",
  "data": { "category": "driver|connect|sql|cancel|internal", "sqlState": "..." }
}
```

- [ ] **Task 1.1:** 写完 `packages/drivers/jdbc/protocol.md`（含版本号 `protocolVersion = 1`）
- [ ] **Task 1.2:** Rust `protocol.rs`：serialize/deserialize + 单测
- [ ] **Task 1.3:** Java 侧对称 DTO / 解析；stderr 日志、stdout 仅 JSON

### 1.2 AgentProcessManager

**Files:** `packages/drivers/jdbc/src/agent_process.rs`

行为：

1. `ensure_running()`：解析 `java`（Settings 覆盖 → `JAVA_HOME` → `PATH`）
2. `Command` spawn：`java -jar <agent.jar>`，stdin/stdout piped，stderr 读入 tracing
3. 发 `agent.hello`，校验 `protocolVersion`
4. 请求队列：**单写者**往 stdin 写；读 stdout 按 `id` 匹配 pending oneshot
5. 空闲计时：无 session 且空闲 ≥ 配置分钟 → `agent.shutdown` 或 kill
6. 子进程 EOF：标记 dead；下一请求允许**自动重启一次**
7. Host 退出：`Drop` / 显式 `shutdown()` 杀进程

- [ ] **Task 1.4:** 实现 spawn + hello + 请求/响应匹配
- [ ] **Task 1.5:** 空闲回收 + 退出清理 + pid 陈旧清理
- [ ] **Task 1.6:** 集成测：用 mock 脚本（Python/Node 回显 JSON-RPC）代替真 JVM

**验收：**

- mock Agent 下 100 次 RPC 无串话
- kill mock 后下一请求自动重启或返回明确错误
- 主测进程结束无残留子进程

---

## Phase 2：Java Agent — 连接、查询、流式

### 2.1 类结构（建议）

```text
AgentMain          // 入口，安装 Security/流重定向
JsonRpcLoop        // 读 stdin 行 → dispatch → 写 stdout
SessionManager     // sessionId → SessionContext
SessionContext     // jars loader, pool 或 pinned Connection, open cursors
DriverLoader       // URLClassLoader + Driver.connect
PoolHolder         // HikariCP per connection identity（url+user+jars hash）
JdbcExecutor       // execute / fetch / cancel / update
MetaService        // DatabaseMetaData 封装
TypeCodec          // ResultSet → JSON 行
```

### 2.2 连接与池

- `session.open`：校验 jars 路径可读 → 创建/复用 ClassLoader → 配置 Hikari（默认 max 8, minIdle 0）
- 短请求：borrow → work → return
- `tx.*` 与开放 `cursorId`：pin 连接；关闭后 evict 或 return（**pin 过的勿借给其他 session**）
- 密码仅存 SessionContext 内存，不写日志

### 2.3 查询流式

```text
query.execute:
  setFetchSize(fetchSize or 500)
  读最多 maxRows
  hasMore → 分配 cursorId，缓存 ResultSet + Statement + pinned Connection
query.fetch:
  继续读；读完 query.close 等价清理
query.cancel:
  Statement.cancel()
```

### 2.4 类型映射（MVP）

| JDBC | JSON / Host Value |
|------|-------------------|
| NULL | null |
| BOOLEAN | bool |
| TINYINT–BIGINT | number 或 string（超大用 string） |
| FLOAT/DOUBLE | number |
| DECIMAL/NUMERIC | string |
| CHAR/VARCHAR/CLOB | string（CLOB 限长度） |
| BINARY/BLOB | base64 string + type tag |
| DATE/TIME/TIMESTAMP | ISO-8601 string |
| 其他 | string 或 `{ "unsupported": true, "type": "..." }` |

- [ ] **Task 2.1:** `session.open/close` + H2 内存库连通
- [ ] **Task 2.2:** `query.execute` / `fetch` / `close` + fetchSize
- [ ] **Task 2.3:** `exec.update` + 基础 `tx.begin/commit/rollback`
- [ ] **Task 2.4:** `TypeCodec` 单测（含 null / decimal / timestamp）
- [ ] **Task 2.5:** 驱动类找不到 / JAR 损坏 / 错误 URL → 可读 error.category

**验收：** 手工 `java -jar agent.jar` + 管道发送 JSON，对 H2 完成 open → select → fetch → close。

---

## Phase 3：JdbcDriver 对接 Host

### 3.1 实现 `DatabaseDriver`

**Files:** `packages/drivers/jdbc/src/driver.rs`

| Trait 方法 | 映射 |
|------------|------|
| `connect` / `test_connection` | `ensure_running` + `session.open` |
| `disconnect` | `session.close` |
| `get_databases` / `get_tables` / `get_table_schema` / columns | `meta.*` |
| `query` / `query_multi` | `query.execute`（无 cursor 或一次取完） |
| `query_stream` | execute + 异步 Stream 内 `fetch` |
| `execute` | `exec.update` |
| `transaction` | `tx.*` |
| `supports_explain` | `false` |
| `migration_renderer` | `None` |
| cancel 相关 | 有 `executionId` 时 `query.cancel` |

连接配置扩展字段（存现有加密 store）：

```text
jdbc_url, driver_class?, jar_ids[] / jar_paths[], props{}
```

- [ ] **Task 3.1:** `JdbcDriver` + `DatabaseDriverFactory` + inventory 注册
- [ ] **Task 3.2:** handle ↔ sessionId 映射与 disconnect 清理
- [ ] **Task 3.3:** `query_stream` 背压：前端停消费时及时 `query.close`
- [ ] **Task 3.4:** 将 `DriverError` 映射到现有 Host 错误展示

### 3.2 前端连接 UI

**Files（按现有连接表单模式对齐）：**

- `packages/drivers/jdbc/ui/meta.ts` — `connectionView: 'sql'` 或专用 `jdbc`，字段定义
- 连接对话框：JDBC URL、用户、密码、Driver Class、JAR 多选
- Settings：已导入 JAR 目录、JRE 路径、Agent 状态、空闲超时

JAR 管理：

1. 用户选择本地 `.jar`
2. 复制到 app-data `jdbc-drivers/<hash>-<name>.jar`
3. 连接只存 id/相对路径，避免用户移动原文件失效

- [ ] **Task 3.5:** `meta.ts` + codegen 进 `DB_REGISTRY`（仅 jdbc feature 开启时）
- [ ] **Task 3.6:** 连接表单字段与校验（URL 必填、至少一 JAR）
- [ ] **Task 3.7:** Settings「JDBC Drivers」：导入/删除/列表；JRE 检测结果
- [ ] **Task 3.8:** i18n keys（en + 可选 zh-CN）
- [ ] **Task 3.9:** 连接列表展示「JDBC（能力有限）」提示文案

**验收（A2）：** 导入 PostgreSQL 或 H2 JAR → 新建连接 → 测连 → 查表 → SELECT 出网格。

---

## Phase 4：元数据、取消、产品化

### 4.1 元数据与事务打磨

- [ ] **Task 4.1:** `meta.databases/tables/columns` 对齐树节点字段；缺失时返回空而非炸 UI
- [ ] **Task 4.2:** 主键信息（若 MetaData 有）挂到 table schema
- [ ] **Task 4.3:** 只读连接：Host 已有 read-only 门闸；Agent 侧 `setReadOnly(true)` 尽力而为
- [ ] **Task 4.4:** `QueryExecutionId` 与 `query.cancel` 贯通（驱动不支持则声明不可取消）

### 4.2 稳定性

- [ ] **Task 4.5:** Agent 崩溃自动重启一次 + 连续失败熔断
- [ ] **Task 4.6:** 应用退出 / 窗口关闭路径无残留 `java` 进程（A6）
- [ ] **Task 4.7:** 大结果集：默认 maxRows/fetchSize 与 Host 流式策略对齐，避免一次物化 10 万行（A5）

### 4.3 打包与文档

- [ ] **Task 4.8:** 构建脚本产出 `datazen-jdbc-agent-all.jar`；版本号独立于主程序
- [ ] **Task 4.9:** 可选 SKU / 下载说明：如何放置 agent.jar 与启用 `jdbc` feature
- [ ] **Task 4.10:** 用户文档草稿 → 实现稳定后移到 `docs/features/jdbc-guide.md`
- [ ] **Task 4.11:** 更新 `docs/architecture/backend/drivers.md` 增加「外部 Agent 驱动」一小节
- [ ] **Task 4.12:** 安全说明：不可信 JAR、凭据、localhost/stdio only

**验收：** PRD A1–A6 清单全部勾选；Basic 无 jdbc 时安装包行为不变。

---

## Phase 5（非 MVP，仅登记）

- URL 前缀推断逻辑类型（展示达梦/Oracle 等）与部分能力开关
- Schema Diff / Data Transfer 最小 JDBC 适配
- 每连接独立 Agent 进程（更强隔离，更高内存）
- Protobuf 替代 JSON-RPC
- 推荐 JAR 校验和（只校验不代下）
- SSH：复用 Host 隧道后把 URL 改写为 `localhost:转发端口`（MVP 可已支持，本阶段做矩阵测试）

---

## 文件变更矩阵（MVP）

| 区域 | 路径 | Phase |
|------|------|-------|
| Registry | `drivers-registry.json` | 0 |
| Rust driver | `packages/drivers/jdbc/**` | 0–4 |
| Java agent | `datazen-jdbc-agent/**` | 0–2, 4 |
| 协议文档 | `packages/drivers/jdbc/protocol.md` | 1 |
| 前端 meta | `packages/drivers/jdbc/ui/meta.ts` | 3 |
| Settings UI | `src/windows/settings/...`（JDBC 段） | 3–4 |
| 连接表单 | 现有 new-connection 管线 + meta 字段 | 3 |
| i18n | `src/locales/en.ts`, `zh-CN.ts` | 3 |
| 架构文档 | `docs/architecture/backend/drivers.md` | 4 |
| 功能文档 | `docs/features/jdbc-guide.md` | 4（完成后） |
| E2E | `packages/drivers/jdbc/e2e/` 或 Host e2e feature gate | 4 |

---

## 测试计划

### 单元

| 测什么 | 哪里 |
|--------|------|
| JSON-RPC 编解码、错误 category | `packages/drivers/jdbc` |
| Agent 状态机、空闲回收 | `agent_process` + mock 子进程 |
| TypeCodec | Java tests |
| DriverLoader 错误路径 | Java tests（坏 jar / 错 class） |

### 集成

| 测什么 | 怎么做 |
|--------|--------|
| hello / open / query / fetch | mock Agent 或 H2 |
| 进程被杀后恢复 | 集成测 |
| 无残留进程 | 断言 pid 不存在 |

### E2E / 手工

- [ ] H2 文件库：导入 JAR → 连接 → 树 → SELECT → 断开
- [ ] 可选：PostgreSQL 官方 JAR（开发机）
- [ ] 无 JRE / 错 JAR / 错 driver class 的 UI 错误
- [ ] 启用 jdbc 与纯 Basic 两种构建冒烟

### CI 建议

- 默认 CI：**不**要求本机 JRE
- `jdbc` job：安装 JRE 17+、构建 agent、跑 H2 集成测
- Host E2E 默认跳过 jdbc，除非 `DATAZEN_E2E_JDBC=1`

---

## 风险与缓解（实施向）

| 风险 | 缓解 |
|------|------|
| 驱动 `System.out` 污染协议 | Agent 启动时重定向 System.out → stderr；解析端忽略非 JSON 行并打 warn |
| ClassLoader / SPI 冲突 | 直接 `Driver.connect`；一 session 一主驱动；文档禁止随意堆多驱动 |
| 大结果 OOM | 强制 fetchSize + maxRows；stream 必须 `query.close` |
| 僵尸 JVM | 退出钩子、启动清 pid、A6 测试 |
| 用户期望过高 | UI 能力提示 + features 文档对比原生驱动 |
| 许可证 | 不分发厂商 JAR；测试用 H2 |

---

## 建议 PR 拆分

```text
PR-0  布局 + registry + hello 通（Rust mock + Java 空 Agent）
PR-1  协议定稿 + AgentProcessManager 完整生命周期
PR-2  Java session/query/fetch/tx + H2 集成测
PR-3  JdbcDriver trait 全量 MVP + 连接 UI + JAR 导入
PR-4  元数据树 + cancel + 回收打磨 + 文档 + 可选 E2E
```

每个 PR 可独立合并；PR-3 起才对终端用户可见「能连」.

---

## 实施顺序（依赖图）

```text
Phase 0 布局
    → Phase 1 协议 + ProcessManager
        → Phase 2 Java 查询核心
            → Phase 3 JdbcDriver + UI
                → Phase 4 元数据/稳定性/文档
                    → Phase 5 增强（可选）
```

---

## 执行入口

1. 评审本方案与 [PRD](./jdbc-driver-support.md) 决策表（IPC / 单 Agent / 仓库布局）。
2. 开 PR-0，勾选 Task 0.x。
3. 按 Phase 顺序推进；Phase 边界做一次手工 H2 验收再进下一阶段。
4. 全部 MVP 完成后：功能说明移入 `docs/features/`，本文件与 PRD 标记完成或归档。

**Worktree 建议：** `scripts/new-feature-worktree.sh jdbc-agent`（若仓库已有该脚本）。

---

## 附录 A：agent.hello 示例

```json
// →
{"jsonrpc":"2.0","id":1,"method":"agent.hello","params":{
  "hostVersion":"0.x.y",
  "protocolVersion":1
}}
// ←
{"jsonrpc":"2.0","id":1,"result":{
  "agentVersion":"0.1.0",
  "protocolVersion":1,
  "capabilities":["jdbc","session","query.stream","tx"]
}}
```

## 附录 B：session.open + query.execute 示例

```json
{"jsonrpc":"2.0","id":2,"method":"session.open","params":{
  "url":"jdbc:h2:mem:test;DB_CLOSE_DELAY=-1",
  "user":"sa",
  "password":"",
  "jars":["/app/data/jdbc-drivers/h2.jar"],
  "driverClass":"org.h2.Driver"
}}
{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s-1"}}

{"jsonrpc":"2.0","id":3,"method":"query.execute","params":{
  "sessionId":"s-1",
  "sql":"SELECT 1 AS n",
  "maxRows":1000,
  "fetchSize":500
}}
{"jsonrpc":"2.0","id":3,"result":{
  "columns":[{"name":"N","type":"INTEGER"}],
  "rows":[[1]],
  "hasMore":false
}}
```

## 附录 C：与 PRD 验收映射

| PRD ID | 本计划覆盖 |
|--------|------------|
| A1 Basic 无 JRE | Phase 0/4 构建矩阵 |
| A2 PG/H2 可查 | Phase 2–3 |
| A3 非内置 JAR | Phase 3 手工 |
| A4 Agent 被杀可恢复 | Phase 1.5 / 4.5 |
| A5 大结果流式 | Phase 2.2 / 4.7 |
| A6 无残留进程 | Phase 1.5 / 4.6 |
