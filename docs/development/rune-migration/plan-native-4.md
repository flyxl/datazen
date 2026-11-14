# Rune 原生宿主第四阶段：basic 驱动接入

前三阶段（[plan.md](plan.md)、[plan-native.md](plan-native.md)、[plan-native-3.md](plan-native-3.md)）交付了演示界面、活宿主与宿主能力分发。本期把桌面版 basic SKU 的驱动集接进原生宿主：**postgres、mysql、sqlite、redis 四个核心驱动**（仓库权威定义见 `scripts/resolve-drivers.mjs` 的 basic 预设；README 的驱动表同一口径）。sqlite 已于 N3 链接，本期补 postgres、mysql、redis 三家。

React 继续保留，规则与前几阶段相同。本阶段不改 `packages/frontend` 与 `packages/web`。

## 差距（写于立项时）

[native-host.md](native-host.md) 收尾时如实写下的两件事，本期逐项处理：

1. **帧循环上的网络读**：结构页四读（DDL/列/索引/外键）在渲染函数里同步调适配器；部件柱状图在 `bar_chart` 渲染里每帧查询；T5 外键导航（出站按键取 FK 清单、入站"被引用"扫描）同步走活会话。本地 sqlite 是有界 IO（N10 判据）；远程驱动是无界网络 IO，必须先进作业通道。
2. **驱动只链接了 sqlite**：`host_registry()` 静态准入一个工厂；postgres/mysql/redis 代码在 workspace 里现成，但既没被宿主链接，也没有静态组装所需的工厂导出。

另有连接表单差距：P4 表单只写内存端口，活宿主没有 `save_connection`——远程连接目前只能靠 T7 导入或手改 `connections.json` 落地。

## 调查结论（写于立项时）

- **basic 的权威定义**：`scripts/resolve-drivers.mjs`——postgres、mysql、sqlite、redis 四个核心 path 驱动；协议兼容家族顺带支持：postgres 侧 questdb、cloudberry（`postgres_alias_factory!`），mysql 侧 mariadb、doris、starrocks、manticore、ob_oracle（mysql crate 自有测试断言六家族工厂）。redis 单工厂。连同 sqlite 共 **11 个工厂 id**。
- **工厂形态**：sqlite 导出 `pub fn factory() -> &'static dyn DatabaseDriverFactory`（静态组装入口，`register_driver!` 提交的是同一工厂）；postgres/mysql/redis 的工厂是 crate 私有 static，只经 inventory 懒注册——静态 `with_factories` 形态需要各 crate 补一个导出（对齐 sqlite 的先例与注释口径）。
- **`DriverRegistry` 词汇现成**：`with_factories(impl IntoIterator<Item = &'static dyn DatabaseDriverFactory>)`（N3 已用）；`available_types()` 列已准入类型——连接表单的引擎白名单由此出，"链接即提供"。
- **`ConnectionConfig` 字段完备**：host/port/database/schema/username/password/ssl_mode/options/read_only——表单所需的远程参数都有落点；密码经 `ConnectionService.save` 落盘时走 T10 的封存，`connections.json` 不落明文的性质自动继承。
- **作业通道模式现成**：`AdapterJob`/`AdapterDelivery` 六种作业已带代次；`ReadTable` 的 UI 模式（begin_job → loading 态 → poll 投递 → 缓存渲染）照抄即可。
- **D 系列 UI 仍走内存端口**（N12 只做了宿主函数 `sync_table`/`compare_table`），无帧循环网络，不阻塞本期。
- **沙箱无数据库服务器**（无 redis-server/postgres/mysqld，无 root 不可安装）：真机端到端冒烟无法在本环境完成，如实记录；离线可验的是工厂准入、类型解析、连接不可达的诚实错误路径，以及仓库 CI 同形的驱动 crate 自身测试套（`scripts/ci-local.sh` 的 basic 步骤：`DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-{postgres,mysql,sqlite,redis} --tests`）。
- **wasm 不受影响**：驱动依赖只进 `datazen-rune-host`（纯 native 宿主），`datazen-ui` 的 wasm 目标（H6）不链接它们。

## 不变量（承接前三阶段，新增两条）

1. 沿用前三阶段全部不变量（见 [plan.md](plan.md)、[plan-native.md](plan-native.md)、[plan-native-3.md](plan-native-3.md)）。
2. **无界 IO 不上帧循环**：任何可能走网络的读，一律经 `AdapterJob` 作业通道带代次投递；渲染函数只读 AppState 缓存，不调适配器。
3. **链接即声明**：宿主声明的引擎集 = `host_registry()` 准入的工厂集；没验证过的能力不写进文档矩阵，真机端到端未验的如实标注。

## 门禁

沿用 [tasks.md](tasks.md) 的 GR1–GR4。G1–G4（前端）不适用：本阶段不改 `packages/frontend`、`packages/web`。新增：

| 编号 | 命令 | 通过标准 |
|---|---|---|
| GR5 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --offline --tests`（E4 起适用；资源不足时降为 `--lib` 并如实记录） | exit 0 |

## 任务

### 作业通道迁移（先搬家，后链接）

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| E1 | 结构读作业化：`AdapterJob::ReadStructure { generation, table }` 一次作业取 DDL+列+索引+外键四合一（worker 经活会话，两个驱动调用一个作业）；UI 打开表时发作业，结构页渲染只读 AppState 缓存（loading/错误态），视图切换与换表失效重来；出站 FK 导航改读结构缓存 | 测：临时库开表后结构缓存填齐四段；渲染函数不再触发适配器调用；换表代次正确，旧代次答案不写缓存；FK 按键在缓存未齐时如实提示 |
| E2 | 部件条作业化：`AdapterJob::WidgetBars { generation, id }`；`bar_chart` 渲染只读缓存；打开仪表盘页、保存部件、手动刷新时发作业 | 测：进仪表盘页后每部件一条 bars 缓存；作业完成前渲染零查询；结果按部件 id 入缓存、代次错配丢弃 |
| E3 | 入站引用作业化 + 同步会话退场：`AdapterJob::InboundRefs { generation, table }`；`load_inbound_refs` 改作业投递；`sync_session`/`SyncSession`/`cached_schema` 的网络路径随之删除（结构缓存由 E1 的作业填充，schema 缓存由 worker 维护） | 测：入站清单经作业投递带代次；宿主 crate 里 `sync_runtime` 只剩 `run_workflow` 回退路径（UI 不可达，注释写明）；全量门禁零回归 |

### 驱动接入

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| E4 | 工厂导出与链接：postgres/mysql/redis 各导出 `pub fn factories()`（postgres 含 questdb/cloudberry 别名，mysql 六家族，redis 单工厂；注释对齐 sqlite 先例），sqlite 补同形 `factories()`；宿主 Cargo 依赖三家驱动 crate；`host_registry()` 准入 11 工厂 | 测：`with_factories` 准入 11 id 全数 admit；`available_types()` 与声明一致；sqlite 既有测试零回归；GR5 首跑（沙箱资源不足则如实降级并记录） |
| E5 | 连接表单接活宿主：live `save_connection`（name/engine/address + username/password/database → `ConnectionConfig` → `ConnectionService.save`，T10 封存自动继承）；表单加三个输入框，engine 白名单取自 `available_types()`；内存端口同签名演进出 `ConnectionDraft` 结构 | 测：postgres 连接经表单落 `connections.json`（密码封存不落明文）；白名单外的 engine 被拒；内存端口往返不回归 |
| E6 | 离线验证扫尾：postgres/mysql/redis 连接不可达路径断言（连本地拒绝端口 → 诚实的连接失败句子，证明驱动真被调用且错误不撒谎）；驱动 crate 测试套按 CI 同形跑（GR5），沙箱跑不动的部分如实记录；真机端到端冒烟明确列为环境差距待验 | 测：三家拒绝路径的错误句子可断言；GR5 结果如实入报告；native-host.md 矩阵措辞与验证程度一致 |
| E7 | 文档收尾：native-host.md 驱动矩阵（四驱动"已链接"、家族清单、验证程度如实分级）、配置目录、能力差距；tasks.md/progress.md 收口 | 测：矩阵声明与测试报告一一对应，无未验声明 |

## 顺序

E1 → E2 → E3（先完成作业通道迁移，帧循环上不再有无界 IO）→ E4（链接）→ E5（表单，依赖 E4 的引擎白名单）→ E6（验证扫尾）→ E7（文档收尾）。

## 范围外（如实写明）

- **真机端到端冒烟**（连真 postgres/mysql/redis 服务器的完整浏览/编辑/查询）：本沙箱无服务器二进制且无 root，无法执行；链接与错误路径已验，端到端待有服务器环境时补。桌面版 CI 的 basic SKU 测试套是同代码的旁证。
- **postgres 双层命名空间的非默认 schema 表寻址**（schema.table 限定名在树/读路径的传递）：E6 的离线验证无法覆盖真 postgres 目录，若真机验证暴露问题，另立任务修，不预写未验代码。
- **其余 12 个 path 驱动与 git 驱动**（kiwi/superset）：同一接缝按需加，`all` SKU 另立。
- **D 系列 UI 接活宿主**（N12 的 `sync_table`/`compare_table` 尚无 UI 接线）：与驱动接入正交，另立任务。
- **SSH/HTTP 隧道、SSL 细节配置**：`ConnectionConfig` 已有字段，表单先提供直连最小集；隧道是独立域（tunnel），不阻塞本期。
