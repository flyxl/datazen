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
命令返回 `alreadyInstalled` 且**不覆盖**用户已有内容。

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

`open` 输出 `datazen://connection/<id>` 描述符和连接摘要，供桌面 App/外部
opener 接手；CLI 本身不启动 GUI。

## 只读与写入开关

默认只读：

- 任何写语句返回 `E_READ_ONLY`；
- `--allow-writes` 只放行 INSERT/UPDATE/DELETE/MERGE/REPLACE 等 DML；
- DDL/管理语句还需要 `--allow-dangerous-sql`，单独使用它不会隐含 `--allow-writes`；
- 连接档案标记 `readOnly` 时写语句始终拒绝；
- 生产连接（`--production`、`environment=production`，或名称/分组 token 为
  `prod`/`production`）即使带两个 flag 也返回 `E_PRODUCTION_WRITE`。

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
| `E_NOT_FOUND` | 连接不存在 |
| `E_CREDENTIAL` | 凭据/加密密钥不可用，拒绝继续 |
| `E_READ_ONLY` | 默认只读策略或连接只读 |
| `E_WRITE_FLAG` | 缺少 `--allow-writes` 或 flag 组合不成立 |
| `E_DANGEROUS_SQL` | 缺少 `--allow-dangerous-sql` |
| `E_PRODUCTION_WRITE` | 生产库写入 |
| `E_TIMEOUT` | 查询/元数据请求超时 |
| `E_DRIVER` | 驱动未编译或连接失败 |
| `E_EXECUTOR` | 执行器装配错误 |
| `E_OUTPUT` | 输出参数/写文件错误 |
| `E_INTERNAL` | 未分类的内部错误（退出码 1） |

退出码依次为：`1` internal、`2` usage、`3` config、`4` not found、`5` credential、
`6` read-only、`7` write flag、`8` dangerous、`9` production write、`10` timeout、
`11` driver、`12` executor、`13` output。

## 边界

- `query`/`schema`/`context` 使用真实共享驱动执行；测试通过可注入
  `QueryExecutor` 覆盖 flag、limit、timeout，不伪造真实查询结果。
- 需要桌面隧道（SSH/HTTP/WebSocket）或平台 vault 的凭据路径尚未在 CLI 装配；
  使用 `DATAZEN_KEYRING=file` 或 `keyring`。这是明确的宿主装配边界，不是静默
  使用假数据。
