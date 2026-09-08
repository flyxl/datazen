# JDBC 驱动支持 PRD（进程外 Agent）

> **状态：** 草案 / 待评审  
> **日期：** 2026-09-08  
> **定位：** 可选扩展，不改变「主程序纯 Rust + 编译期驱动」核心架构  
> **参考：** DBX Agent 模式、Chat2DB-Rust supervised Java engine、现有 `DatabaseDriver` trait

**Goal:** 让 DataZen 在不嵌入 JVM、不破坏轻量与编译期驱动模型的前提下，通过**进程外 JDBC Agent** 连接仅有 JDBC 驱动的数据库（国产库、老旧商业库、厂商强制 JAR 场景）。

**Architecture:** 主进程保持纯 Rust；新增可选 `jdbc` 驱动实现 `DatabaseDriver`，通过本地 IPC 代理到独立 Java Agent 子进程；Agent 动态加载用户提供的厂商 JAR + HikariCP；上层 UI / Workflow / AI / MCP 无感。

**Tech Stack:** Tauri v2 / Rust (`packages/driver-api`) / 独立 Java 17+ Agent（fat-jar 或 jlink） / JSON-RPC 2.0 或 length-prefixed Protobuf / 可选 `DATAZEN_DRIVERS=jdbc`

---

## Global Constraints

- **主程序默认不依赖 Java**：Basic SKU 与默认构建不含 JDBC 路径；安装包体积目标不变。
- **禁止进程内嵌 JVM / JNI**：与「拒绝不稳定动态库 ABI、保持轻量」一致。
- Driver 契约仍走现有 `DatabaseDriver` trait；不新增平行的连接/查询 API 给上层。
- Host 测试写在 `src-tauri/`、`src/**/__tests__/`、`e2e/specs/`；Agent / JDBC 专属测试放在 Agent 工程或 `packages/drivers/jdbc/`。
- 用户导入的 JAR 视为不可信代码；文档与安全边界必须写清。
- 厂商 JDBC 驱动 **不打包进 DataZen**（版权与体积）；用户自行下载并导入。
- i18n 开发期仅改 `src/locales/en.ts` 与可选 `zh-CN.ts`。
- 错误统一走 `DriverError` / `CommandError`；不向 UI 泄漏凭据或完整 JAR 路径中的敏感段。

---

## 1. 背景与动机

### 1.1 问题

DataZen 当前仅支持编译期原生 Rust 驱动（postgres / mysql / sqlite / redis + 可选 path drivers）。大量场景只有 JDBC：

- 国产库：达梦 DM、人大金仓、GaussDB 部分发行版、崖山、虚谷、GBase 等
- 老旧 / 企业库：DB2、Informix、部分 Oracle 兼容模式、厂商定制 JAR
- 组织强制使用官方 JDBC（合规、审计、特性对齐）

用户若无法用原生驱动连接，只能换工具，与「轻量但可扩展」定位冲突。

### 1.2 竞品对照

| 项目 | Java 支持方式 | 主程序体积影响 |
|------|----------------|----------------|
| DBX | 进程外 Agent（JSON-RPC）+ 可选 JDBC 插件 | 主程序 ~20MB，Java 按需 |
| Chat2DB 正式版 | 进程内 Java + SPI | 安装包大 |
| Chat2DB-Rust | Rust 运行时 + 受监督 Java 兼容引擎 | 混合过渡 |
| DataZen（现状） | 无 | 始终纯 Rust |

**结论：** 采用与 DBX 相近的**进程外 Agent**，同时严格适配 DataZen 已有的 `DatabaseDriver` 与编译期 registry，是成本与架构一致性的最佳折中。

### 1.3 非目标（本 PRD 外）

- 进程内 JVM / JNI / 嵌入式 JRE 常驻
- 把所有现有原生驱动改为走 JDBC
- 在 Agent 内复刻完整 Schema Diff MigrationRenderer / 方言级高级能力（MVP 以「能连、能查、能基础元数据」为主）
- 自动从 Maven Central 下载任意 JAR（可后续做白名单目录；MVP 用户本地导入）
- 多租户远程 Agent 或 Agent 集群

---

## 2. 目标与成功标准

### 2.1 MVP 目标

- [ ] 可选构建 / 安装「JDBC Agent」扩展，主程序可不含 Java
- [ ] 用户可导入厂商 `.jar`，创建「JDBC」类型连接（URL + 用户/密码 + 可选 driver class）
- [ ] 实现 `DatabaseDriver` 代理：`connect` / `test_connection` / `disconnect`、`get_databases` / `get_tables` / `get_table_schema` / `get_columns`、`query` / `query_multi` / 基础 `query_stream`、`execute`、基础事务（若驱动支持）
- [ ] Agent 进程懒启动、空闲回收、主进程退出时清理
- [ ] 连接失败、驱动类找不到、JAR 损坏等给出可读错误
- [ ] Settings：JRE 检测 / 引导、JAR 列表管理、Agent 状态
- [ ] 文档：安全边界、导入 JAR、能力限制说明

### 2.2 成功标准（验收）

| ID | 标准 |
|----|------|
| A1 | Basic 构建且未启用 jdbc 时，安装包与启动路径无 JRE 依赖 |
| A2 | 使用 PostgreSQL 官方 JDBC JAR，可完成连接、列表库表、执行 SELECT、看到结果网格 |
| A3 | 使用达梦或 H2 等至少一种「非内置原生」JAR，同样完成 A2 |
| A4 | 杀死 Agent 进程后，下次查询可自动重启 Agent 或给出明确「Agent 已退出」错误 |
| A5 | 大结果集（≥10 万行量级，可配置 limit）以流式/分批回传，不一次性撑爆内存 |
| A6 | 断开连接与应用退出后无残留 Java 进程 |

### 2.3 非功能目标

- Agent 冷启动到可接受连接：目标 < 3s（本机已有 JRE 与 JAR 缓存）
- 单连接空闲默认 5–15 分钟回收 Agent（可配置）
- 协议与实现需支持后续升级到 Protobuf 而不破坏 MVP JSON-RPC（若首版选 JSON）

---

## 3. 架构设计

### 3.1 总体结构

```text
React / TypeScript
        │ Tauri IPC
        ▼
DataZen Host (Rust)
  commands / services / ConnectionManager
        │
        ▼
JdbcDriver : DatabaseDriver          ← packages/drivers/jdbc（或等价 path）
  AgentProcessManager（启动、健康、回收）
        │  localhost IPC
        │  JSON-RPC 2.0 或 length-prefixed Protobuf
        ▼
Java Agent 子进程（可选组件）
  URLClassLoader(用户 JAR)
  HikariCP
  ResultSet → 行批次编码器
        │
        ▼
目标数据库
```

### 3.2 与现有 Driver 体系的衔接

1. **Registry**  
   在 `drivers-registry.json` 增加可选条目，例如：

   ```json
   {
     "jdbc": {
       "source": "path",
       "path": "packages/drivers/jdbc",
       "feature": "driver-jdbc",
       "description": "Generic JDBC via external Agent"
     }
   }
   ```

   - 默认 Basic **不包含** `jdbc`
   - 文档说明：`DATAZEN_DRIVERS=basic,jdbc` 或发行 SKU「+JDBC」

2. **实现**  
   `JdbcDriver` 实现 `DatabaseDriver`；`driver_type()` 可为 `jdbc` 或根据 URL/用户选择的「逻辑类型」做展示用映射（能力仍按 JDBC 通用能力声明）。

3. **能力声明**  
   - `supports_explain`：默认 false，除非后续做方言探测  
   - `supports_offset`：谨慎默认 true，失败时降级  
   - `migration_renderer` / 高级 Schema Diff：MVP **不提供**（返回 Unsupported）  
   - `query_stream`：必须实现分批，避免 `query_multi` 全量物化

### 3.3 Agent 进程模型

| 策略 | MVP 选择 | 说明 |
|------|----------|------|
| 启动时机 | 懒启动 | 首次 JDBC `connect` / `test_connection` 时启动 |
| 进程数量 | 单共享 Agent（推荐） | 简化；连接级隔离可作为 Phase 2 |
| 回收 | 空闲超时 + 应用退出 | `ConnectionManager` 无 JDBC session 且空闲超时后 kill |
| 崩溃恢复 | 下一请求检测并重启一次 | 连续失败则报错，避免重启风暴 |
| JRE | 检测 `JAVA_HOME` / `java`；可选引导下载精简 JRE | **不**强制捆绑进主安装包 |

### 3.4 协议（MVP 建议）

**MVP 可用 JSON-RPC 2.0 over stdio 或 TCP localhost**（实现快，与 DBX 心智接近）。

核心方法草案：

| Method | 方向 | 说明 |
|--------|------|------|
| `agent.hello` | H→A | 版本、协议号、能力 |
| `driver.load` | H→A | JAR 路径列表、可选 main driver class |
| `session.open` | H→A | JDBC URL、user、password、props → `sessionId` |
| `session.close` | H→A | |
| `meta.databases` / `meta.tables` / `meta.columns` | H→A | 基于 DatabaseMetaData |
| `query.execute` | H→A | SQL、limit、流式 cursor 配置 |
| `query.fetch` | H→A | 续取 batch |
| `query.cancel` | H→A | 尽量映射 Statement.cancel |
| `exec.update` | H→A | 写语句 |
| `tx.begin` / `commit` / `rollback` | H→A | |

**安全：** 密码仅出现在 `session.open` 参数中，不写 Agent 日志；Agent 日志默认 redact。

**版本：** 协议带 `protocolVersion`；不兼容时 Host 拒绝并提示升级 Agent。

### 3.5 Java Agent 内部

- Java 17+；单 fat-jar 或 `jlink` 自定义 runtime（发行策略二选一，PRD 不强制）。
- 动态加载：对用户选定 JAR 使用独立 `URLClassLoader`（注意驱动 SPI 与父子加载器陷阱）。
- 连接池：HikariCP；短请求借还连接；显式事务 / 服务端游标场景固定连接直到 `session.close` 或 cursor 结束。
- 类型映射：JDBC → DataZen `Value`（null / bool / int / float / string / bytes / timestamp / json）；未知类型安全降级为 string 或明确 Unsupported。
- 不实现业务 UI；纯协议服务端。

### 3.6 前端与配置

**连接表单（JDBC 类型）：**

- 显示名、JDBC URL、用户名、密码
- Driver JAR（多选 / 列表，来自「已导入驱动」）
- Driver Class（可选；现代驱动可自动注册）
- 高级：连接属性键值、只读开关、SSH（复用现有隧道，Agent 连本地转发端口）

**Settings：**

- 已导入 JAR 管理（添加、删除、显示路径/大小/哈希可选）
- JRE 路径覆盖
- Agent 启用开关、空闲超时、日志级别
- Agent 运行状态（pid / 上次错误）

**Store：** 连接配置中 JDBC 字段进入现有加密存储；JAR 文件存 app-data 下受控目录（复制导入，避免用户事后移动路径失效）。

---

## 4. 能力范围与降级

### 4.1 MVP 必须

- 连接与测试连接
- 库 / 表 / 列等基础元数据（DatabaseMetaData）
- SELECT 与多结果集的有限支持
- 分页或 limit 下的结果展示
- 简单 DML/DDL execute（受只读连接与 Safe Mode 约束）
- SSH 隧道下的 localhost JDBC URL

### 4.2 MVP 明确降级或不做

| 能力 | MVP |
|------|-----|
| Schema Diff 方言渲染 | Unsupported |
| Data Sync 同族高级路径 | 不保证；可后续按逻辑类型开放 |
| EXPLAIN 可视化 | 不保证 |
| 完整对象树（过程/触发器等） | 尽力 DatabaseMetaData，缺失则空 |
| 存储过程调试、厂商专有管理面 | 不做 |
| 与原生驱动对等的所有 command_definitions | 仅标准 query/execute + 基础 catalog |

UI 上对 JDBC 连接展示「通用 JDBC，高级能力有限」提示，避免用户以为与原生 postgres 驱动对等。

---

## 5. 分阶段实施

### Phase 0：协议与骨架（约 1 周）

- [ ] 确定 IPC 形态（stdio JSON-RPC vs TCP Protobuf）与 `protocolVersion`
- [ ] 空 Agent：hello / 健康检查 / 优雅退出
- [ ] Rust `AgentProcessManager`：spawn、stdin/stdout 帧、超时、kill
- [ ] 集成测试：Host 拉起 mock Agent 完成 hello

### Phase 1：连接与查询 MVP（核心）

- [ ] `driver.load` + `session.open/close`
- [ ] `query.execute` + `query.fetch` 分批
- [ ] `JdbcDriver` 实现 connect / query / query_stream / execute
- [ ] 前端 JDBC 连接表单 + 导入 JAR
- [ ] 用 H2 或 PostgreSQL JDBC 做端到端手工验收（A2）

### Phase 2：元数据与事务

- [ ] databases / tables / columns / primary keys
- [ ] 基础 begin/commit/rollback
- [ ] 只读连接与 cancel（能 cancel 则接 `QueryExecutionId`）
- [ ] 错误码与用户可读消息映射

### Phase 3：产品化

- [ ] Settings 完整 UI、JRE 检测与文档链接
- [ ] 空闲回收、崩溃重启策略、无残留进程保证
- [ ] 安全文档与「不可信 JAR」提示
- [ ] E2E：导入 JAR → 创建连接 → 查询（可用 H2 文件库，避免外网）
- [ ] `drivers-registry` / 可选 SKU / 打包脚本（Agent jar 与主程序分离分发）

### Phase 4（后续，不在 MVP）

- 按 URL 前缀推断逻辑数据库类型，部分能力增强
- Schema Diff / Data Transfer 的最小 JDBC 适配
- 连接级 Agent 隔离
- 可选官方「推荐 JAR」校验和（仅校验，不代下）
- Protobuf 协议与性能优化

---

## 6. 文件与模块预估

| 区域 | 路径（建议） | 说明 |
|------|----------------|------|
| Driver | `packages/drivers/jdbc/` | `JdbcDriver` + 注册 |
| Agent 管理 | `src-tauri/src/db/jdbc_agent.rs` 或 `services/` | 进程生命周期 |
| 协议 | `packages/drivers/jdbc/protocol.md` + 生成代码可选 | 契约文档 |
| Java Agent | 独立目录或独立仓库 `datazen-jdbc-agent/` | 可单独发版 |
| UI | 连接表单 + `Settings` JDBC 段 | |
| 文档 | `docs/features/jdbc-guide.md`（实现后） | 本文件为 todo PRD |
| Registry | `drivers-registry.json` | `jdbc` 可选条目 |

Java Agent **建议独立版本号**，与主程序解耦（对齐 DBX「插件版本不必等于主程序版本」）。

---

## 7. 安全与合规

1. **信任边界：** 用户导入的 JAR = 在本机执行的任意字节码；安装与导入 UI 必须明确提示。
2. **凭据：** 仅 Host 加密存储；传输到 Agent 使用本地 IPC；Agent 不落盘密码。
3. **进程权限：** Agent 以当前用户权限运行；不要求提权。
4. **网络安全：** Agent 默认只听 localhost 或纯 stdio；禁止默认绑定 `0.0.0.0`。
5. **供应链：** DataZen 不分发第三方商业 JDBC；文档给出「请从厂商官网获取」指引。
6. **审计：** 可选记录「使用了 JDBC Agent + driver 哈希」，不含 SQL 入参中的秘密。

详见实现时对齐 `docs/architecture/security.md` 的更新小节。

---

## 8. 测试计划

### 8.1 自动化

| 层级 | 内容 |
|------|------|
| Rust 单元 | 协议编解码、Agent 状态机、空闲回收逻辑 |
| Rust 集成 | mock Agent 进程；真实 H2 JAR（测试夹具，注意许可证） |
| Java Agent 单元 | 类型映射、池化借还、cancel |
| E2E | 仅在启用 jdbc feature 的 job：导入 H2 → 查询 → 断开 → 无残留进程 |

### 8.2 手工

- [ ] PostgreSQL JDBC
- [ ] 至少一种国产或非原生库 JAR（达梦 / 金仓 / H2 文件模式）
- [ ] 错误 JAR / 错误 driver class / 无 JRE
- [ ] SSH 隧道
- [ ] 应用强杀后的进程清理

---

## 9. 风险与缓解

| 风险 | 严重度 | 缓解 |
|------|--------|------|
| 用户期望与原生驱动能力对等 | 高 | UI 明确标注能力边界；文档对比表 |
| ClassLoader / SPI 冲突 | 高 | 独立 ClassLoader；单驱动优先；文档「一连接一主驱动」 |
| 大结果集 OOM | 高 | 强制分批 fetch；Host 侧 limit 与现有流式模型对齐 |
| Agent 僵尸进程 | 中 | 退出钩子、pid 文件、启动时清理陈旧 pid |
| JRE 碎片与版本 | 中 | 明确最低 17；检测并提示 |
| 维护两套运行时 | 中 | Agent 独立发版；协议版本化；MVP 范围锁死 |
| 许可证（H2 测试、用户 JAR） | 中 | 测试夹具选许可清晰组件；不分发厂商 JAR |

---

## 10. 决策记录（待评审确认）

| 决策点 | 建议默认 | 备选 |
|--------|----------|------|
| IPC | stdio JSON-RPC 2.0 | TCP + Protobuf |
| 进程模型 | 单共享 Agent | 每连接一 Agent |
| 分发 | Agent 独立下载/可选组件 | 与 All SKU 捆绑（仍不捆绑厂商 JAR） |
| 逻辑库类型 | MVP 固定 `jdbc` | URL 推断展示名 |
| Schema Diff | MVP 不做 | Phase 4 最小支持 |

---

## 11. 实施入口

1. 评审本 PRD（架构 + 安全 + 打包）。
2. 锁定 Phase 0 协议与仓库布局（Agent 是否 monorepo）。
3. 按 Phase 0 → 1 → 2 → 3 开 PR；每个 Phase 可独立合并。
4. 实现完成后：将使用说明移入 `docs/features/`，本文件标记完成或移出活跃 todo。

**相关文档：**

- `docs/architecture/backend/drivers.md`
- `docs/development/independent-driver-development.en.md`
- `docs/development/optional-drivers.md`
- `packages/driver-api`（`DatabaseDriver` trait）
