# Rune 原生宿主第三阶段：宿主能力与分发

第一阶段（[plan.md](plan.md)）交付了演示数据上的全部界面，第二阶段（[plan-native.md](plan-native.md)）把活宿主接了出来：sqlite 全功能经既有用例真跑。第三阶段补齐宿主能力（剪贴板、文件对话框、托盘、钥匙串）、剩余可离线验证的用例域（值搜索、外键导航、收藏、连接导入、片段持久化、工作流引擎），以及分发（安装包与发布流水线）。

React 继续保留，规则与前两阶段相同。本阶段不改 `packages/frontend` 与 `packages/web`。

## 差距（写于立项时）

[native-host.md](native-host.md) 收尾时如实列出的剩余差距，本阶段逐项处理：

1. **宿主能力**：文件对话框（导出/备份路径目前固定在数据目录）、系统剪贴板（网格与结果网格无复制）、托盘、加密密钥存储（`connections.json` 的密码明文落盘）。
2. **用例域**：值搜索、外键导航、收藏、连接导入未接；片段（P5 端口）在原生宿主仍走内存；工作流引擎只真跑 query 步，command/condition/foreach/merge/transform 报 not wired。
3. **打包**：只有裸二进制，无安装包与发布流水线。

## 调查结论（写于立项时）

- **rune 自带三项宿主能力**，不需要在宿主 crate 里另引依赖：剪贴板（`Tx::copy`/`copy_rich`，`Runtime::use_clipboard` 供测试注入）、文件对话框（`native-file-dialog` 特性：macOS/Windows 走 rfd，Linux 走 XDG desktop portal over zbus；`FileDialogs::scripted` 供测试注入）、托盘（`Tray`/`TraySpec`，Linux 需 dbus 特性，无会话总线时如实报错）。
- **锁文件已就绪**：zbus 6.2.1、rfd 0.16/0.17、keyring 4.1.6（桌面 crate 已用）均在 workspace 锁文件与离线缓存中，开特性不需要新解析。
- **datazen 已有词汇**：`services/value_search`（逐表 `search_table_values`）、`services/referenced_by`（`list_inbound_foreign_keys`）、`history/domain/favorite`（`FavoriteQuery`，桌面形态=favorites 根目录下的 `.sql` 文件，`id`=文件名词干、ULID 由应用铸造）、`connection_import`（dbeaver/datagrip/navicat/tableplus/dbx/datazen 六种格式）、workflow 引擎（`WorkflowExecutor::execute_with_options`，宿主只需实现四方法 `WorkflowHost` trait，condition/foreach/merge/transform 由引擎内部处理）。
- **桌面钥匙串语义**：`keyring` v4（`DATAZEN_KEYRING`/`DATAZEN_TEST_KEYRING` 环境变量，见 `crates/datazen-desktop/src/store/key_backend.rs`），原生宿主复用同一 crate 与同一语义。

## 不变量（承接前两阶段，新增三条）

1. 沿用前两阶段全部不变量（见 [plan.md](plan.md)、[plan-native.md](plan-native.md)）。
2. 宿主能力一律先接缝后真机：测试走 rune 的 scripted/memory 注入或诚实错误路径；不 mock 出不存在的功能，不做空接线。
3. 钥匙串与托盘在无会话总线/无钥匙串的环境如实报不可用；钥匙串不可用时**不**静默降级为明文存储。
4. wasm 构建保持可编译（H6 结论）：桌面专属特性（`native-file-dialog`、dbus、keyring）按 target 门控，不进 wasm 目标。

## 门禁

沿用 [tasks.md](tasks.md) 的 GR1–GR4。G1–G4（前端）继续不适用：本阶段不改 `packages/frontend`、`packages/web`。

## 任务

### 宿主能力

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| T1 | 系统剪贴板：表格网格与结果网格的选中格 Ctrl+C 复制为 TSV 文本（经 `Tx::copy`），编辑器内复制沿用 rune 内建 | 测：选中 2×2 格复制，`use_clipboard` 注入的 memory 板读回 TSV；无选中不写剪贴板 |
| T2 | 文件对话框：开 rune `native-file-dialog` 特性（非 wasm target 门控）；导出/备份经 `save_file` 选保存路径，导入向导经 `open_file` 选输入文件；对话框不可用或取消时如实提示，不落错文件 | 测：scripted 后端注入路径后导出落在所选路径；取消则不写文件并提示；wasm 目标仍可编译 |
| T9 | 托盘：宿主启动时装 `TraySpec`（图标+菜单：显示窗口/退出），事件路由到窗口与退出；无会话总线时启动照常、托盘缺席并记日志 | 测：spec 构造单测；无总线环境 `Tray::new` 的错误被吞掉且宿主初始化照常完成 |

### 用例域

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| T3 | 片段持久化：宿主 `snippets.json` 文件仓储（原子写）接 P5 片段端口 | 测：增改删经端口往返后文件在、重启仍在 |
| T4 | 值搜索：值搜索面板（值+匹配模式），对当前连接逐表跑 `search_table_values`（作业化），结果清单（表.列、命中数）点击开表并带过滤 | 测：临时库两表种值 → 搜索命中正确；无命中空清单如实显示 |
| T5 | 外键导航：出站 FK 单元格"前往引用行"（打开引用表按引用列过滤）；入站 FK"被引用"（`list_inbound_foreign_keys`）清单打开引用表带过滤 | 测：两表 FK 的临时库，导航落到引用行；被引用清单列出真实引用表 |
| T6 | 收藏：`FavoriteQuery` 按桌面语义存 favorites 根目录下的 `.sql` 文件（`id`=文件名词干，ULID 铸造，改名不改文件）；保存当前 SQL/列表/打开/删除 | 测：保存后磁盘 `.sql` 文件形状对；列表/打开/删除往返；改名不改文件名 |
| T7 | 连接导入：`connection_import::parse_import_file` 六格式解析 → 预览清单 → 确认后建入文件连接存储（重名去重） | 测：真实格式样例文件解析出连接并建立；坏文件如实报错不落库 |
| T8 | 工作流引擎真跑：宿主实现 `WorkflowHost`（connection_name 走 profiles、execute_command 走带 deadline 的进程执行、complete_ai 如实报"AI 未配置"、sqlite 标准方言），`WorkflowExecutor` 走 N1 作业通道，步骤结果仍进 W7 报告 UI | 测：含 query/command/condition/foreach 的定义在临时库全步骤真结果；AI 步报"未配置"；命令超时被 deadline 掐断 |

### 钥匙串与分发

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| T10 | 加密钥匙串：连接密码经 `keyring` v4 存 OS 钥匙串（service `datazen-native`，语义对齐桌面 `key_backend`），`connections.json` 不再存明文密码；钥匙串不可用时连接页如实报错 | 测：设/取往返；钥匙串缺席时连接加载如实报"密码不可用"而非明文回退。若 libdbus 链接在本环境不可行，任务降级为文档化差距并如实记录 |
| T11 | 打包分发：`.deb`（dpkg-deb，control+图标+桌面项）；GitHub Actions 发布流水线（打 tag → 三平台构建 → 产物上传）；文档更新 | 测：`.deb` 在沙箱内可构建可解包检查；流水线 YAML 与仓库既有 workflow 同构；native-host.md 如实更新 |

## 顺序

T1 → T2（先把 UI 层能力接稳）→ T3 → T4 → T5 → T6 → T7（用例域逐个接线，T7 依赖 T2 的 open_file）→ T8（引擎）→ T9 → T10（钥匙串风险后置）→ T11（收尾打包）。

## 范围外（如实写明）

- **多窗口**：rune 支持（一个事件循环、多窗口宿主），但当前没有面板需要独立 OS 窗口；React 版的 WindowChrome/MenuBar 是自绘 chrome 而非 OS 多窗。等具体面板需要脱离主窗时另立任务。
- **AI、MCP、监控、隧道**：外部服务端配置才能验证，维持第二阶段结论，不做空接线。
- **WAPPS 管理与 Onboarding 向导**：永久留在 React（第一阶段 C4 结论）。
- **postgres/mysql 等其余驱动**：接缝与 N3/N4 相同，按需逐个加，不阻塞本系列。
- **远程驱动的作业化**：接入远程驱动时把部件查询与工作流步骤移到 N1 作业通道（T8 的引擎路径已在作业线程内）。
