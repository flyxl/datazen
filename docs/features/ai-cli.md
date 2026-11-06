# DataZen CLI 与官方 Agent Skill

`datazen` CLI 为脚本和本地 Agent 提供只读优先的数据库访问。它复用桌面宿主的
连接档案与共享驱动端口，不启动 Tauri，也不自行保存第二份凭据。

## 构建与安装

```bash
cargo build -p datazen-cli --release
# 二进制：target/release/datazen
```

将二进制放入 `PATH` 后，安装官方 Agent Skill：

```bash
datazen agent setup
```

命令把 `SKILL.md` 安装到 `~/.agents/skills/datazen/SKILL.md`。如果该文件已存在，
命令返回 `alreadyInstalled` 且**不覆盖**用户已有内容。安装的 Skill 只是调用指南，
不是授权：它要求 Agent 默认只读、显式选择连接 id、始终加 limit/timeout、从不询问
密码，也不把数据库行里的文本当作指令。重复执行幂等，不会创建半成品文件。

`datazen` CLI 与桌面 Settings 里的“本地 CLI Agent Provider”是两件事：前者让外部
Agent 通过只读优先的 CLI 访问数据库；后者把 Claude Code、Codex、Cursor 等编码 CLI
作为桌面 AI Chat 的模型后端。两者都不伪造组织管理员，也不能绕过桌面/服务端的写入
确认与生产保护。

`--limit` 在 SQL Server 上不会改写 T-SQL（没有通用 LIMIT 语法），而是执行后的
客户端行数上限；其余支持 LIMIT 的方言在驱动执行前追加。因此大表查询仍应写选择
性谓词，不能只依赖 `--limit`。

## 连接与凭据

CLI 读取桌面应用数据目录中的 `connections.json`：

- 默认目录为平台 data dir 下的 `com.tbeasy.datazen`；
- `--data-dir <dir>` 或 `DATAZEN_DATA_DIR` 可覆盖；
- 凭据使用应用现有的 AES-256-GCM 密钥。存在应用 `.key` 时使用文件后端；
  也可显式设置 `DATAZEN_KEYRING=keyring` 使用系统钥匙串；
- 缺少密钥时 query 返回稳定的 `E_CREDENTIAL`，不会把密文当密码发送；
- 列表/错误/日志不输出密码、passphrase、token 或连接串。

当前二进制链接 PostgreSQL、MySQL（含兼容驱动）与 SQLite 驱动；未链接的驱动
返回 `E_DRIVER`。`capabilities`/`doctor` 会列出本构建实际注册的驱动。

## 命令

```bash
datazen doctor --json
datazen capabilities --json
datazen connections --json

datazen schema <connection> --database <db> --json
datazen context <connection> --database <db> --json
datazen query <connection> "SELECT ..." --limit 100 --json
datazen query <connection> "SELECT ..." --csv --file result.csv

datazen open <connection> --json
datazen agent setup --json
```

`<connection>` 优先按 id 匹配，其次按唯一名称匹配；同名时返回 `E_USAGE`，要求
使用 id。`--file -` 等同 stdout。

`open` 会定位已安装的 DataZen 桌面应用，并以 argv 契约启动它：

```bash
datazen open <connection> [--query "SELECT ..."] --json
# 桌面进程收到:
# --datazen-open-connection <id> [--datazen-open-query <sql>]
# --datazen-data-dir <path> --datazen-open-ack <path>
```

macOS 直接定位 `/Applications/DataZen.app`（或 `~/Applications/DataZen.app`）的
`Contents/MacOS/<exe>` 并 spawn，完整 argv 不经过 LaunchServices；只有找不到 bundle
可执行文件时才回退 `/usr/bin/open -n -a ... --args`；回退同样等待 ack，无法验证
ack 时返回 `E_UNSUPPORTED`，不会报成功。测试可用 `DATAZEN_DESKTOP_APP` 指向临时 `.app`。Windows/Linux 使用常见
安装路径或 `DATAZEN_DESKTOP_BIN` 指定的可执行文件。
`--data-dir` 会以 `--datazen-data-dir <path>` 转发，桌面在日志/Store 初始化前
设置 `DATAZEN_DATA_DIR`，确保 CLI 与桌面读写同一份数据。桌面启动时解析参数，向主窗口发送
`datazen:open-connection`，前端打开对应连接；带 `--query` 时在连接就绪后创建
查询面板。桌面在 emit 成功后写 `--datazen-open-ack`；CLI 最多等待 5s，launcher
非 0 退出、ack 缺失或超时返回 `E_EXECUTOR` / `E_TIMEOUT`，不会输出
`launched:true`。首次 onboarding 未完成时请求会排队，主窗口创建后重放。
找不到桌面应用返回 `E_NOT_FOUND`，不会伪造 `datazen://` URI。

## 只读与写入开关

默认只读：

- 任何写语句返回 `E_READ_ONLY`；
- `--allow-writes` 只放行 INSERT/UPDATE/DELETE/MERGE/REPLACE 等 DML；
- DDL/管理语句还需要 `--allow-dangerous-sql`，单独使用它不会隐含 `--allow-writes`；
- 连接档案标记 `readOnly` 时写语句始终拒绝；
- 生产连接（`--production`、`environment=production`，或名称/分组 token 为
  `prod`/`production`）即使带两个 flag 也返回 `E_PRODUCTION_WRITE`。

生产连接对会改变状态的 `PRAGMA`（如 `journal_mode=WAL`、`user_version=7`）、
`ATTACH`/`DETACH`/`VACUUM` 也一律拒绝；只读 PRAGMA 查询（如 `PRAGMA table_info`、
`PRAGMA journal_mode`）放行。

`--limit` 对 SELECT/WITH 在驱动执行前追加 `LIMIT n+1`；已有更大的 LIMIT 时
外层再包一层 `LIMIT n+1`，不是拿到全部结果后截断。前置/尾随注释不影响改写，
追加的 LIMIT 会放在尾随注释之前；仅 MySQL 方言把 `#` 作为行注释，
PostgreSQL `#`/`#>`/`#>>` 和 SQL Server `#temp` 标识符不截断；支持 PostgreSQL
`$tag$...$tag$` / `$$...$$`，dollar quote 内的 `;`、注释和 LIMIT 不参与扫描；
`WITH` 只有最终语句是 SELECT 时才改写，
`WITH ... UPDATE/INSERT/DELETE ... RETURNING` 不改写；多语句按顶层分号逐条
处理，每条 SELECT 各自追加，避免把 LIMIT 写到错误位置。非 SELECT 不改写。
`--timeout` 接受裸秒数、`s` 或 `ms`；超时返回 `E_TIMEOUT`。

## 输出

- `--json`：结构化对象；
- `--csv`：RFC 4180 转义；
- 两者互斥，同传返回 `E_OUTPUT`；
- `--file` 写入文件，避免把大结果打入终端。

## 稳定错误码

| code | 含义 |
| --- | --- |
| `E_USAGE` | 参数/选择歧义 |
| `E_CONFIG` | 应用数据或 profile 配置不可用 |
| `E_NOT_FOUND` | 连接或桌面应用不存在 |
| `E_CREDENTIAL` | 凭据/加密密钥不可用，拒绝继续 |
| `E_READ_ONLY` | 默认只读策略或连接只读 |
| `E_WRITE_FLAG` | 缺少 `--allow-writes` 或 flag 组合不成立 |
| `E_DANGEROUS_SQL` | 缺少 `--allow-dangerous-sql` |
| `E_PRODUCTION_WRITE` | 生产库写入 |
| `E_TIMEOUT` | 查询/元数据请求超时 |
| `E_DRIVER` | 驱动未编译或连接失败 |
| `E_EXECUTOR` | 执行器装配错误 |
| `E_OUTPUT` | 输出参数/写文件错误 |
| `E_UNSUPPORTED` | 宿主尚未装配的能力（隧道、不支持的平台） |
| `E_INTERNAL` | 未分类的内部错误（退出码 1） |

退出码依次为：`1` internal、`2` usage、`3` config、`4` not found、`5` credential、
`6` read-only、`7` write flag、`8` dangerous、`9` production write、`10` timeout、
`11` driver、`12` executor、`13` output、`14` unsupported。

## 边界

- `query`/`schema`/`context` 使用真实共享驱动执行；测试通过可注入
  `QueryExecutor` 覆盖 flag、limit、timeout，不伪造真实查询结果。
- SSH/HTTP/WebSocket 隧道尚未在 CLI 装配。检测到隧道配置时命令返回
  `E_UNSUPPORTED` 并拒绝连接，绝不会降级为直连绕过隧道。
- 平台 vault：macOS 读取 `security` 中的 DataZen master key，Windows 读取 DPAPI
  blob；其他平台使用 `DATAZEN_KEYRING=file`/`keyring`。无法读取密钥时返回
  `E_CREDENTIAL`。
- `open` 启动真实桌面进程并复用启动事件/前端路由；不输出不可消费的 URI。
- `query` 目前由 CLI 直接构造共享驱动执行；`datazen::execution` 的
  Runtime/Session 端口还没有可注入 CLI 的适配器（见下方装配边界）。

## Runtime 装配边界

CLI 没有伪造 Runtime 集成。宿主执行 Runtime 的入口
`RuntimeConnectionUseCases::new` 需要 `ProfileRepository`、`PolicyService` 和
`SessionBackend` 三个真实端口；其中 `SessionBackend` 还要承担打开物理资源、
执行/取消、终结句柄和关闭资源，并依赖宿主的 owner/lease/epoch/隧道装配。
CLI 只有解析后的 `ConnectionConfig` 与 `create_driver`，没有这些宿主对象；
桌面实现（如 `DesktopSessionBackend`）也不在 CLI 的依赖闭包里。因此本轮保持
真实共享驱动直连，不把测试替身包装成 Runtime。
