# datazen-native：纯原生宿主

`datazen-native` 是共享 Rune UI 的**活宿主**二进制：`crates/datazen-rune-host` 的 bin 目标，不依赖 Tauri，连接、读表、编辑、查询全部走 `datazen` 的既有用例与真驱动。它与 Tauri 版（`datazen-desktop`，React 界面）并行存在——React 不下线，Rune 是另一套界面（第一阶段 [plan.md](plan.md)，第二阶段 [plan-native.md](plan-native.md)）。

## 构建与运行

```bash
cargo build --release -p datazen-rune-host
./target/release/datazen-native
```

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
| `migration-runs.json` | 同步/传输/对比的运行记录 | N12 |
| `workflows.json` | 工作流定义（registry 用例的行存储） | N13 |
| `dashboards.json` | 仪表盘与部件行 | N13 |
| `exports/` `backups/` | 导出与备份产物 | N11 |
| `result-artifacts/` | 执行 runtime 的结果工件 | N5 |

## 驱动支持矩阵

宿主经 `datazen` 的驱动 registry 解析连接（静态 `with_factories` 形态，`live.rs` 的 `host_registry()`）。**本期只链接 sqlite，sqlite 全功能**（连接、目录、读表、编辑提交、查询执行、结构页、导出、备份、同步/对比/传输、部件查询）。其余驱动按同一接缝扩展——`host_registry()` 的工厂清单加一行，数据路径（N4/N6/N7 的用例）无需改动：

| 驱动 | 桌面版 | datazen-native |
|---|---|---|
| sqlite | 支持 | **支持（本期）** |
| postgres / mysql / sqlserver / duckdb / mongodb / redis / clickhouse / elasticsearch / influxdb / victoriametrics / turso / rqlite / hbase / vector / http 系 | 支持 | 未链接（同一接缝，按需加入） |

## 功能面与剩余差距（如实）

已接（经既有用例）：

- **连接**：文件存储、列表、连接打开（作业化，不阻塞帧循环）
- **数据**：对象树真 catalog、读表页（分页/排序/过滤真 SQL）、脏格提交（preview→fingerprint→commit）
- **查询**：编辑器语句经执行 runtime、结果网格、取消作废代次、查询历史
- **结构**：DDL/列/索引/外键从真 catalog 读
- **工具**：导出 CSV、备份、两连接间单表同步、结构对比、传输（作业化进度）
- **设置**：主题/页大小/最大行数持久化
- **片段**：增改删经宿主文件存储持久化（T3），内存端口仅作演示回退
- **工作流与仪表盘**：registry 用例持久化、部件经 `create_widget_from_sql`、步骤与部件查询经执行用例真跑（N13）

剩余差距（本系列范围外，如实列出）：

- **宿主能力**：托盘、系统菜单、多窗口、加密密钥存储（文件对话框与系统剪贴板已由 T1/T2 接线：导出/备份/导入经系统对话框选路径，网格与编辑器走系统剪贴板）。
- **用例域**：AI、MCP 审批、监控、隧道、值搜索、外键导航、收藏、连接导入、语言包、admin——需要外部服务或宿主能力，另立任务，不做空接线。
- **工作流引擎**：宿主路径真跑 query 步；command/ai/condition/foreach/merge/transform 步报告 not wired（完整引擎的 grants/plan guard/资源块是独立任务）；部件运行历史同属引擎侧。
- **WAPPS 管理与 Onboarding 向导**：永久留在 React（第一阶段 C4 结论）。
- **远程驱动的作业化**：部件查询与工作流步骤目前在同步路径上（本地 sqlite 的有界 IO 类，N10 同判据）；远程驱动接入时需移到 N1 的作业通道。
