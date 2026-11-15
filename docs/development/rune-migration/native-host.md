# datazen-native：纯原生宿主

`datazen-native` 是共享 Rune UI 的**活宿主**二进制：`crates/datazen-rune-host` 的 bin 目标，不依赖 Tauri，连接、读表、编辑、查询全部走 `datazen` 的既有用例与真驱动。它与 Tauri 版（`datazen-desktop`，React 界面）并行存在——React 不下线，Rune 是另一套界面（第一阶段 [plan.md](plan.md)，第二阶段 [plan-native.md](plan-native.md)）。

## 构建与运行

```bash
cargo build --release -p datazen-rune-host
./target/release/datazen-native
```

### 打包（T11）

- **`.deb`**：`packaging/native/build-deb.sh [release|dev]` —— release 二进制 + desktop 项 + 应用图标 + control（`Depends: libxkbcommon0`），`dpkg-deb --root-owner-group` 产出 `target/deb/datazen-native_<版本>_<架构>.deb`；版本取 git describe（tag→净版本）。
- **发布流水线**：`.github/workflows/release-native.yml` —— 打 `v*` tag 或手动触发，Linux（.deb + 发布）/macOS（tar.gz）/Windows（zip）三平台构建 `datazen-native`，产物上传到该 tag 的草稿 release（与 Tauri 版 `release.yml` 同惯例：softprops v3、draft、单一 notes 写者、并发组）。React/Tauri 桌面仍走 `release.yml`，两条流水线互不触碰。

- 工具链：Rust stable（CI 同款）。
- Linux 依赖：`libxkbcommon`（winit 事件层）。无显示环境（无 X/Wayland）时窗口初始化会失败——这是图形应用的预期行为。
- 产物裁剪：workspace release 剖面（`strip` + `lto` + `codegen-units=1` + `opt-level=z"`），x86_64 Linux 实测约 36 MB。

## 配置目录

所有文件在 `${XDG_DATA_HOME:-$HOME/.local/share}/datazen-native/` 下；`DATAZEN_NATIVE_CONNECTIONS` 指定连接文件路径时，其余文件落在该路径的**同级目录**。

| 文件/目录 | 内容 | 接线任务 |
|---|---|---|
| `connections.json` | 连接存储（原子写）；`DATAZEN_NATIVE_CONNECTIONS` 可整体覆盖路径 | N2 |
| `settings.json` | UI 设置（主题、页大小、最大行数），`ui` 键下 | N8 |
| `query-history.json` | 查询历史（保留最近 1000 条） | N9 |
| `snippets.json` | SQL 片段存储（原子写） | T3 |
| `favorites/` | SQL 收藏（每条一个自述 `.sql` 文件，删除进 `.trash/`） | T6 |
| `migration-runs.json` | 同步/传输/对比的运行记录 | N12 |
| `workflows.json` | 工作流定义（registry 用例的行存储） | N13 |
| `dashboards.json` | 仪表盘与部件行 | N13 |
| `exports/` `backups/` | 导出与备份产物 | N11 |
| `result-artifacts/` | 执行 runtime 的结果工件 | N5 |
| `.key` | 密码加密主密钥（`DATAZEN_NATIVE_KEYRING=file` 时；keyring 后端正常时密钥在 OS 钥匙串，此文件删除） | T10 |

## 驱动支持矩阵

宿主经 `datazen` 的驱动 registry 解析连接（静态 `with_factories` 形态，`live.rs` 的 `host_registry()`）。**第四阶段链接 basic SKU 全集：四驱动、十一工厂 id**（`scripts/resolve-drivers.mjs` 的 basic 预设同一口径），数据路径（N4/N6/N7 与 E1–E3 的用例、作业通道）对四家同构：

| 驱动 | 家族（同一协议/工厂） | 桌面版 | datazen-native | 验证程度（如实） |
|---|---|---|---|---|
| sqlite | sqlite | 支持 | **已链接** | **全功能离线验证**：连接/目录/读表/编辑提交/查询/结构四读/入站扫描/部件查询/导出/备份（真 sqlite 文件夹具，GR3+GR5） |
| postgresql | postgresql、questdb、cloudberry（别名工厂） | 支持 | **已链接** | 离线验证：工厂准入（E4）、表单落盘与密码封存（E5）、连接不可达诚实句（E6）、驱动 crate 测试套（GR5）；**真机端到端待验**（沙箱无服务器） |
| mysql | mysql、mariadb、doris、starrocks、manticore、ob_oracle（六家族工厂） | 支持 | **已链接** | 同 postgresql 的离线验证集；**真机端到端待验** |
| redis | redis | 支持 | **已链接** | 同上；需真服务器的活测试套在无服务器环境诚实跳过；**真机端到端待验** |
| 其余 path 驱动（sqlserver/duckdb/mongodb/clickhouse/elasticsearch/influxdb/victoriametrics/turso/rqlite/hbase/vector/http 系） | — | 支持 | 未链接（同一接缝：`host_registry()` 加一行；`all` SKU 另立） | — |

"已链接"= 工厂静态准入、连接可解析、表单可建连接；**不等于"已支持"**——真 postgres/mysql/redis 服务器上的完整浏览/编辑/查询冒烟未验，上表分级如实。

## 功能面与剩余差距（如实）

已接（经既有用例）：

- **连接**：文件存储、列表、连接打开（作业化，不阻塞帧循环）、连接导入（T7：六格式解析→预览→确认建入，重名去重；加密格式带密码）、**连接表单接活宿主**（E5：六字段草稿，引擎白名单=注册表准入，密码落盘封存自动继承）
- **数据**：对象树真 catalog、读表页（分页/排序/过滤真 SQL）、脏格提交（preview→fingerprint→commit）
- **查询**：编辑器语句经执行 runtime、结果网格、取消作废代次、查询历史
- **结构**：DDL/列/索引/外键四读一作业（E1：结构页与 ER 画布只读缓存，开表/切视图/DDL 应用重取，代次作废）
- **外键导航**（T5/E3）：出站 FK 从结构缓存按键导航；入站"被引用"清单（`list_inbound_foreign_keys`）经 `InboundRefs` 作业投递（代次+表名双守卫）开引用表带过滤
- **部件条**（E2）：柱状图只读缓存，进页/保存部件/手动刷新带代次发作业；宿主同步 bars 路径封死
- **工具**：导出 CSV、备份、两连接间单表同步、结构对比、传输（作业化进度）、值搜索（T4：逐表逐列 `search_table_values`，命中清单定位开表带过滤）
- **设置**：主题/页大小/最大行数持久化
- **片段**：增改删经宿主文件存储持久化（T3），内存端口仅作演示回退
- **收藏**（T6）：保存当前查询/列表/打开/改名/删除，桌面同格式 `.sql` 文件（front-matter 元数据+语句），改名不改文件，删除进 `.trash/`
- **工作流与仪表盘**：registry 用例持久化、部件经 `create_widget_from_sql`、步骤与部件查询经执行用例真跑（N13）

**帧循环零无界 IO**（E1–E3 不变量）：结构四读、部件查询、入站扫描全部经 `AdapterJob` 作业通道带代次投递，渲染函数只读 AppState 缓存；宿主侧同步网络路径已删（`sync_runtime` 仅剩 UI 不可达的工作流回退）。

剩余差距（本系列范围外，如实列出）：

- **宿主能力**：系统菜单、多窗口（文件对话框/系统剪贴板 T1/T2、托盘 T9、加密钥匙串 T10、打包分发 T11 均已接线：连接密码以 OS 钥匙串主密钥 AES-GCM 封存，`connections.json` 不再落明文密码，`DATAZEN_NATIVE_KEYRING=file` 为无钥匙串环境的逃生口；`.deb` 与三平台发布流水线就绪）。
- **用例域**：AI、MCP 审批、监控、隧道、语言包、admin——需要外部服务或宿主能力，另立任务，不做空接线。
- **工作流引擎**（T8 后）：宿主实现 `WorkflowHost` 四方法，`WorkflowExecutor` 经作业通道真跑 query/command（query 与 list_tables/get_schema/describe_table 目录读）/condition/foreach/merge/transform 步；AI 步如实报"未配置"；命令按引擎 deadline 拒绝过期。计划文档里"进程执行"的说法已更正为引擎的真实词汇（七类读工具）。grants/plan guard/资源块与部件运行历史仍属引擎侧后续任务。
- **WAPPS 管理与 Onboarding 向导**：永久留在 React（第一阶段 C4 结论）。
- **真机端到端冒烟**（E6 环境差距）：postgres/mysql/redis 连真服务器的完整浏览/编辑/查询——链接、表单落盘、不可达错误路径已离线验证，端到端待有服务器环境补验，验前不写"支持"。
- **postgres 非默认 schema 的 schema.table 寻址**：离线验证无法覆盖真 postgres 目录，真机验证若暴露问题另立任务修，不预写未验代码。
- **隧道与 SSL 细节**：连接表单先提供直连最小集（`ConnectionConfig` 字段已有落点）；SSH/HTTP 隧道是独立域（tunnel）。
- **D 系列工具页 UI 接活宿主**（N12 的 `sync_table`/`compare_table` 尚无 UI 接线）：与驱动接入正交，另立任务。
