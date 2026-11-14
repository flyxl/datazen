# Rune 原生宿主接线（第二阶段）

第一阶段（[plan.md](plan.md)）交付了共享 Rune UI 和演示数据上的全部界面。第二阶段把 `datazen-rune` 之外的**活宿主**接出来：真数据、真文件、真驱动，让纯原生二进制具备 Tauri 版的核心功能。

React 继续保留，规则与第一阶段相同。本阶段不改 `packages/frontend` 与 `packages/web`。

## 差距（写于立项时）

Tauri 宿主有 353 个 invoke 命令、27 个应用模块、真驱动与加密存储。Rune 侧 `DesktopAdapter` 只有 3 个方法且全部指向内存演示世界。差距分三层：

1. **数据与用例接线**：查询执行、结构页、同步/对比/传输、历史、设置持久化、导出、备份全部跑在内存端口上。
2. **宿主能力**：文件存储、真实驱动会话、异步执行（现在 `futures_executor::block_on` 只对内存 future 安全）。
3. **打包**：只有裸二进制，无 release 产物与配置说明。

## 怎么拆

沿用第一阶段规则：一个任务最多一天，一个提交只做一个任务，feat 提交带门禁结果，tester 报告写进 `testing/NXn-*.md`，`tasks.md` 与 `progress.md` 同步，任务结束立即 push。

## 不变量（承接第一阶段，新增三条）

1. 沿用第一阶段全部不变量（见 [plan.md](plan.md)）。
2. 活宿主的驱动一律经 `datazen` 内置 registry/inventory 解析，不在宿主里再写一套注册表。
3. UI 帧循环不阻塞：任何真实 IO 走 N1 的作业通道，`block_on` 只允许出现在作业线程内。
4. 宿主胶水只进 `datazen-rune-host`；`datazen-ui` 与 `datazen-ui-api` 不新增真实驱动依赖。

## 门禁

| 编号 | 命令 | 通过标准 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 全过 |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 退出码 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 全过（N2 起适用） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 退出码 0（N2 起适用） |

G1–G4（前端）继续不适用：本阶段不改 `packages/frontend`、`packages/web`。

## 任务

### 接线地基

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| N1 | 作业接口：`DesktopAdapter` 增加 `begin_job`/`poll_jobs`（默认不受理，既有适配器零改动）；读表、提交、查询请求先问作业，受理则网格保持上一页并显示载入中；`app_poll` 投递，代次过期丢弃 | 测：受理型适配器两帧后换新页；过期代次不写状态；未受理路径行为与 H4/H5 相同 |
| N2 | 宿主 crate `datazen-rune-host`（bin `datazen-native`）：`datazen_ui::run_with_adapter` 注入活适配器；JSON 文件 `ConnectionRepository`（原子写）接 `from_repository` 接缝 | 测：文件往返经 use case 生效；空文件列空列表。`datazen-native` 起来后连接来自文件而非演示种子 |

### 真数据

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| N3 | 活适配器开真 sqlite 会话（驱动经 lib registry 解析）；对象树读真 catalog | 测：临时库两张表 → 树列出真表名；坏路径文件显示错误不崩 |
| N4 | 读表页走 `table_browsing::api::read_table_page`（真驱动），作业化投递；分页/排序/过滤由真 SQL 生效 | 测：60 行临时库第 2 页是真数据；排序过滤生效 |
| N5 | 提交脏格走 `table_editing` 用例（真 UPDATE），失败脏标记保留 | 测：提交后读回新值；约束失败时脏标记还在 |
| N6 | 查询执行走真驱动查询路径，结果进结果网格；取消作废代次 | 测：SELECT 返回真行；错误 SQL 显示错误；取消后结果不进网格 |

### 持久化

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| N7 | 设置经 `settings` use case + 文件仓储持久化；P1 端口接真后端 | 测：页大小/主题写进文件，重启仍在 |
| N8 | 查询历史经 `history` use case + 文件仓储；执行过的 SQL 进历史 | 测：add 后 recent 读回同一条 |
| N9 | 结构页（DDL/列/索引/外键）从真 catalog 读 | 测：带索引和外键的表，清单与建表语句一致 |
| N10 | 导出结果为 CSV 文件（export 用例）；备份落文件（backup 用例） | 测：CSV 行列数对；备份文件存在且非空 |
| N11 | 两个真 sqlite 连接间的同步/结构对比/传输走既有用例 | 测：源库一张表同步后目标库有数据；对比发现缺失列 |
| N12 | 工作流与仪表盘经既有用例 + 文件存储持久化 | 测：建一个部件后重启进程仍在 |

### 收尾

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| N13 | release 构建 + 文档：`cargo build --release -p datazen-rune-host`；README/文档写明 `datazen-native` 用法、配置目录、驱动支持矩阵与剩余差距 | release 产物存在；文档如实写明本期 sqlite 全功能、其余驱动按同一接缝后续扩展 |

## 范围外（如实写明）

- AI、MCP、监控、隧道：需要外部服务端配置才能验证，另开任务，不在本系列里做空接线。
- WAPPS 管理与 Onboarding 向导：不变量 6 与 C4 的结论不变，留在 React。
- postgres/mysql 等其余驱动的活接线：接缝与 N3/N4 相同，按需逐个加，不阻塞本系列。
- 托盘、系统菜单、多窗口：宿主能力，后续单列任务。
