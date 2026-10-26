# 分阶段实施与验收（原 §10）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §10 的全部内容；阶段速览见总纲 §3。

使用 **D0–D8** 编号，避免与平台产品计划的 P0–P10 混淆。每阶段可以有多个小 PR，但每个 PR 必须构建可用、有真实消费者和测试；不先提交一堆空接口等待别人接线。

旧源码注释中引用的 P0/P2/R2/D2 等仍指旧版断环与端口决策，不自动映射成本节新阶段；迁移对应代码时同步更新这些注释和守卫依据。

共同要求：行为兼容、依赖无环、安全检查不减少、目标路径与测试一起移动。改守卫时先增加等价新规则和失败自证，再退役只适用于旧布局的规则。

## D0：建立可信基线

**输入**：当前 HEAD、已安装驱动集合、未提交差异、CI 配置。

**工作**：

1. 核对所有 Cargo 成员和依赖闭包；记录现有边界门禁、测试收集范围及 feature 集合。
2. 为连接/表浏览/执行链路记录入口、协议、持久化副作用与错误行为。
3. 跑现有定向测试；缺少的兼容行为先增加测试，再移动代码。
4. 基线耗时、内存/资源计数、已有失败记录在 PR 验证说明中，不另建长期进度文件。

**退出**：每条已有失败有真实命令/退出码/原因分类；无凭据进入说明；不能把历史绿灯当本轮基线。

**回滚**：基线测试无业务副作用。新增测试保留，不因后续移动删除断言。

## D1：新 modules 与第一条可运行小切片

**输入**：D0；优先从依赖闭包小的共享 Settings 用例开始，而非一次移动所有功能。

**工作**：

1. 建立 `modules/` Cargo 成员、模块级 application/domain、最小 shared。
2. 按[§5.5](02-legacy-crate-migration.md)移动必要类型，旧包只再导出；接入现有桌面 Settings 适配和命令，不只交目录。
3. 实现空 features 与该功能 feature 构建；新模块不反向依赖待退役包。
4. 扩展 `scripts/lib/cargoWorkspace.mjs`、`scripts/check-ddd-layers.mjs`、`scripts/check-platform-crate-boundaries.mjs` 与对应自证，使新成员/内部模块真正进入扫描。
5. 更新 `scripts/run-platform-crate-tests.mjs` 的收集与 feature 矩阵；不能让 datazen-modules 只有空 feature 测试而漏掉业务。
6. 对已迁移模块新增[§4](01-target-architecture.md)的内部边界检测。旧 crate 规则仍覆盖未迁移部分。

**退出**：能实际读写设置；旧/新 import 指向同一类型；禁止依赖的合成样例会失败；扫描目标或测试模块为空时不能报告全绿。

**已核实的 D1 设置小切片（2026-10-25）**：`datazen-modules::settings` 已拥有
`SharedSettings`、`AppSettings` 的 opaque 读写契约、枚举校验与连接池大小归一化，
Desktop 的 `get_shared_settings` / `save_shared_settings` 以及完整设置读写命令均经
`SettingsService` 执行。`src-tauri/src/platform/settings.rs` 是 Desktop 仓储适配器，
复用既有 `DesktopRepository` 的缓存、JSON 格式、原子写入和错误边界；Redis 刷新、
monitor reload 与 tray 同步仍由原命令路径执行。

**D1 收尾清理已完成（2026-10-26）**：`datazen_platform_api::settings::*`、
`platform_api::ports::SettingsRepository`、`datazen_application::settings` 过渡导出
与 `ApplicationServices` 的设置门面字段均已删除。`SettingsService` 改为持有
`Arc<dyn SettingsRepository>`（宿主可在装配根长期共享同一服务实例）；桌面宿主的
`PlatformAdapter` 直接持有模块设置服务并以 `require_settings_service` 暴露，
命令经 `platform::ipc::require_settings_service` 取用例。`datazen-platform-api`
与 `datazen-feature-core` 显式依赖 `datazen-modules`（settings feature）以直接
消费设置词汇。设置事实仍只有一份：模块服务与宿主持有的是同一个仓储实例。

**回滚**：revert 本切片恢复旧入口，设置文件格式不变。

## D1.5：AI crate 收拢与 infra 改名（2026-10-26 追加决策）

> 本阶段由用户三条架构原则追加（详见[§5.4](02-legacy-crate-migration.md)与[§3.2](01-target-architecture.md)），
> 在 D2 之前执行，避免后续阶段对着即将消失的 crate 布线。

**输入**：D1；ai-api / ai-providers / platform-infra 的消费者清单。

**工作**：

1. `ai-api` 并入 `ai-providers`：trait、类型、工厂注册移入后者（保持 `datazen-ai-api`
   的导出路径语义由 `datazen-ai-providers` 统一承担），更新全部消费者 import。
2. 合并后的 `ai-providers` 整体并入 `modules/src/ai/`：契约（trait/类型/工厂）入
   `ai` 契约层，Provider 适配与共享 HTTP 机制入 `ai` 内部 providers 实现；
   删除 `packages/ai-api`、`packages/ai-providers` 两个 crate 与 workspace 成员。
3. `packages/platform-infra` 改名 `packages/infra`（crate 名 `datazen-infra`），
   更新 workspace、消费者与门禁脚本中的名字。
4. 审计 drivers / modules / web / desktop 四类消费者：任意两者共用的无业务工具
   （路径安全、凭据脱敏、原子写入等）收拢进 infra；仅单消费者使用的留在原处。

**退出**：AI Provider 契约与实现只有 `datazen-modules::ai` 一份；旧 crate 名不再
出现在生产依赖；infra 改名后全仓构建与门禁通过；被收拢的工具在原消费者处只剩
单一 import，无重复实现。

**已完成事实（2026-10-26）**：
1. `ai-api` → `ai-providers`（`46dc15ba6`）：契约三文件随迁，导出面等价。
2. `ai-providers` → `modules/src/ai/`（`e02d60d2c`）：domain 契约 + providers 适配
   + registry + 集成测试（`#![cfg(feature = "ai")]` 门控）；F-09 退役、F-14 豁免
   reqwest、F-04 允许垫底层传递边；feature-core / src-tauri 经
   `datazen-modules` 的 `ai` feature 消费。
3. `platform-infra` → `infra`（本提交）：目录、crate 名（`datazen-infra`）、
   LAYERS/F-11/F-04/F-12、TESTED_LAYERS、panic-policy 目标、CI 文档全部同步。
4. 共享审计结论：`redaction`（feature-core / modules::ai / desktop 三类消费）、
   `path_safety`（feature-core / desktop 两类）已在本层；`drivers/http-support`
   仅驱动单类消费，按「单消费者留原处」规则不迁移；驱动无凭据日志路径，
   无需新增 redaction 消费；web 宿主 D7 落地后再按同一规则审计。

**回滚**：整体 revert 恢复三个 crate；不涉及持久化格式变更。

## D2：连接领域与宿主仓储

**输入**：D1；Profile / ConnectionCatalog 现有契约和调用方清单。

**工作**：

1. connection domain 统一配置、ID、revision 与仓储责任；application 提供 CRUD、导入、摘要及执行配置投影。
2. 把无 scope 的旧连接目录消费者改为可信上下文调用，移除重复 ConnectionCatalog 定义。
3. 抽取 DesktopConnectionStore，复用同一缓存、锁和密钥；把目录与 Profile 适配接到同一存储事实。
4. 兼容既有连接表单、分组、排序、部分更新与 CAS，不把原有凭据语义改成空值覆盖。
5. 收口前端连接命令、导入命令以及 AI/MCP 的目录读取；后续 AI/MCP 整体迁移在 D6。

**退出**：同一契约测试通过真实桌面适配与 fake；跨组织不可见；并发保存不覆盖新 revision；普通摘要不带凭据；连接 UI 旅程保持可用。

**回滚**：恢复旧适配调用，不需要转换配置文件。不得在旧新实现之间双写。

**第一切片已完成（2026-10-26）**：`datazen-modules::connection` 落地
（domain：`ConnectionScope` / `ConnectionRepository` / `ConnectionRepositoryError`；
application：`ConnectionService`，吸收表单回显的密钥合并、脱敏投影与复制流程，
含 7 条用例测试）。桌面实现 `src-tauri/src/platform/connections.rs` 的
`DesktopConnectionStore`（复用同一 `DesktopRepository` 的缓存、写锁与密钥）。
`platform-api::ports::ConnectionCatalog` 与 feature-core 的重复 trait 定义均已删除
（PORT_TRAITS / §6.4 端口表 / inventory 解析清单同步）；`ApplicationServices`
不再持有 connections 字段；连接命令、分组命令、导入/导出、schema diff 校验、
AI 工具（egress 摘要 / db_tools / 连接上下文）与 MCP 的目录读取全部改为经
`require_connection_service` 消费模块服务（无 scope 的旧消费者改为显式作用域）。

**第二切片已完成（2026-10-26）——Profile/revision 词汇统一**：
- `modules::shared` 建立（常驻，不设 feature 门）：全部 ID newtype、`RequestContext`
  /`OwnerRef`/`DelegationRef`、`PortError`/`ApiError` 错误信封唯一定义于此
  （platform-api 的 `id`/`context`/`error` 改为再导出垫片，约 210 个消费者路径不变）。
- `modules::connection::domain::profile`：`ProfileScope`/`ProfileRecord`/
  `ProfileDraft`/`ProfilePatch`/`ProfileRepository`/`ProfileReader` 归位
  （读写端口同文件、按读写切分不合并；源码自检测试随迁）。
- `modules::connection::domain::target`：`NamespaceTarget`/`TargetNamespaceLayer`
  归位；驱动能力元数据（shape/case/requirements）与 `ExecutionTarget` 留在
  platform-api，随 D3/D4 迁移。
- `modules::connection::application::profile_view`：`ProfileView` 与投影
  `ProfileView::from_record`（原 `ProfileRecord::to_view`，响应形状归 application，
  domain 不反向依赖）；`is_sensitive_option` 脱敏黑名单随迁。
- platform-api `ports/profile`/`ports/profile_reader`/`dto/profile`/`target` 改为
  再导出垫片；PORT_TRAITS 清单口径扩展为「`pub trait` 声明或 modules 再导出」
  两种事实源（inventory 解析器新增再导出识别）。
- 桌面 `DesktopProfiles` 与 `DesktopConnectionStore` 已共用同一
  `DesktopRepository` 存储事实（platform_profile 读写路径），无第二份存储。
- 4 个 `to_view()` 调用点（runtime operations、桌面 session_commands ×3）改为
  `ProfileView::from_record(&…)`。

## D3：Execution 拆分与资源机制归位

**输入**：D2；既有连接管理和 fake-runtime 测试契约。

**工作**：

1. 拆开 RuntimeConnectionUseCases 的应用编排、领域状态和私有机制。
2. 迁移所需 DTO/机制类型的完整依赖闭包。若包含 Job 契约、缓存或预算基础类型，先迁其拥有者的必要部分，不能让新 execution 反向依赖旧 kernel。
3. runtime 私有化，经模块装配入口创建；旧 facade 委托同一个新服务。
4. 保留目标执行、会话执行、事务、取消、幂等、资源预算、idle 与 artifact 行为。
5. 封装等待能力，按[§6.4](03-execution-and-jobs.md)验证竞态；生命周期装配接入桌面关停。

**退出**：[§12](09-acceptance-tests.md)的执行用例及现有资源测试通过；同一 operation 只执行一次；失败/取消/关停均无资源泄漏；owner/epoch/revision 校验完整。不能仅以 cargo check 代替生命周期验证。

**回滚**：以干净进程切回旧代码。会话不跨版本持久化迁移；不能运行中把旧句柄交给新资源表。

## D4：Schema、表读写与 SQL 消费者

**输入**：D3；现有表浏览 transport、分页与写回行为测试。

**工作**：

1. schema 应用服务和私有缓存归位；缓存 key 包含现有组织/连接/目标/revision 等隔离维度，不只按表名缓存。
2. 按[§9](06-first-slice-table-browsing.md)完成表浏览完整链路，删除重复业务算法，兼容旧 IPC 映射。
3. table_editing 迁移 ChangeSet、预览、提交、行身份与 fingerprint，复用 execution 事务边界。
4. SQL 编辑器/结果写回接入应用入口，保留风险确认、只读与取消规则。
5. 真实方言测试留在对应 driver crate；公共 UI 行为仍在 Host E2E。

**退出**：读页、筛选/排序、keyset/count、连续编辑、提交/回滚与目标漂移旅程通过；不能因读接口改造破坏下一步写入。

**回滚**：旧新前端 wire 兼容；已产生的数据修改按原事务语义处理，不能把代码 revert 当数据回滚。

## D5：Jobs 与迁移三件套

**输入**：D3/D4；既有计划消费、checkpoint、fencing 与恢复测试。

**工作**：

1. 完成 jobs domain/application/engine 和宿主 JobRepository 接线。
2. 迁移 Schema Diff，再迁 Data Sync、Data Transfer；每个功能单独交付 Profile → plan → review → execute → history 的完整路径。
3. Handler 由各功能实现，宿主注册；Job 引擎不 import 具体功能。
4. 三件套各自拥有仓储 trait；跨三类 Profile 的读取用已定义窄投影，不能恢复总 Store。
5. 计划 revision、唯一消费、预算、取消、部分成功和恢复事实保持不变；迁移共享算法落 migration_common。

**退出**：重复提交不会重复应用；旧 revision/权限撤销被拒绝；中断后恢复不复用过期会话；每个三件套有真实驱动和 UI 旅程证据。

**回滚**：持久化计划和历史不变；已经产生的外部 DB 副作用不能靠 revert 撤销，必须按既有恢复/补偿流程处理。

## D6：其余功能与旧宿主业务移除

**输入**：前述基础功能稳定；每批只迁一个功能族。

迁移顺序及边界：

1. Settings 剩余路径、Named Views、History：共享业务进各模块；窗口位置、桌面路径等仍是宿主专属配置。
   **Named Views 已完成（2026-10-26）**：`datazen-modules::named_views`（domain：DTO/规则/端口/IdMinter；
   application：三个用例 + `NamedViewService` 门面）。`SortCondition` 随迁 `driver-api::filters`
   （feature-core 经再导出保持旧路径）。feature-core 的 `services::named_views` 模块删除，
   `ApplicationServices` 移除 named_views 字段，命令经 `require_named_views_service` 消费。
2. Dashboard：保存/组件创建/运行历史/导入导出规则进入 application/domain，管理库实现留宿主。文件对话框留宿主交互层，应用服务接收已选择的内容/受控文件能力，不能把 GUI 对话框作为 Web 必填依赖。
3. Workflow：保留 step/session/transaction block 的资源边界，调用 execution 与 jobs。
4. AI / MCP：Provider 契约与适配已在 D1.5 并入 `modules::ai`；本阶段迁入的是 feature-core 中 Provider-neutral 的业务（会话编排、工具循环、安全策略）与 MCP 业务分发；认证/stdio/HTTP 登录/事件传输在宿主，授权数据库工具走同一执行入口。
5. Monitor、Export、Backup/Restore、Wapps、Tunnel：按各自模型迁移，保留 grant、文件安全、恢复锁、沙箱桥与网络限制。

每个功能的验收都包含取消/失败、宿主依赖缺失、当前协议兼容和一次用户旅程。最后移除 DesktopRepository 的领域业务方法；它若只剩装配与基础设施，改为用途明确的宿主对象或删除。

**回滚**：每个功能单独 revert；不随重构变更用户数据布局。

## D7：真实 Web 装配与功能选择验收

**输入**：最小 connection/execution/schema/table-browsing 模块集合可独立构建。

**工作**：

1. 新建 web 宿主，注册 `datazen-web`，同步包分类、CI 和平台文档；不再同时创建 server/ 作为第二宿主。
2. 按既有 Web 安全设计接认证、RequestContext、真实 Repository 与执行依赖。
3. 复用相同应用服务，完成连接与表浏览端到端；选择不启用至少一个可选模块，验证无需提供它的仓储。
4. 分开构建两个宿主的 feature 集合，校验无 Tauri 泄入 Web/共享层。
5. 对 Web 实际提供的功能运行真实存储契约、租户/角色、错误映射、断连/取消与关停测试。

**退出**：最小双宿主链路成立，未提供的功能显式声明；完整团队产品仍须满足平台 P7/P8 等相应交付门槛。没有真实 Web 集成结果，不能把 D7 标完成。

**回滚**：回退 Web 服务版本和配置；如首次引入管理库迁移，使用独立评审的 expand/contract 与恢复方案，不由 DDD 目录调整代替。

## D8：旧包退役与发布收尾

**输入**：所有计划迁移的生产消费者和测试都使用新模块，D7 范围明确。

**工作**：

1. 删除旧 application/platform-api/runtime/kernel/feature-core 及已并入的迁移/tunnel 包的重复实现、兼容再导出和 workspace 成员。
2. 更新 Cargo.lock、驱动测试的 dev-dependency、CI 发现、构建脚本、Rustdoc 链接与正式架构说明。
3. 删除临时例外；旧包名仅允许出现在明确的历史说明中，不能出现在生产依赖或可执行测试路径里。
4. 旧 IPC 是否删除单独按外部兼容政策决定；保留委托命令不等于保留旧 crate。
5. 对全部受支持 feature 集合和发布形态执行[§13–§14](10-checks-and-release.md)，不以一个全功能构建掩盖最小集合缺依赖。

**退出**：只剩目标模块和明确保留的独立插件/工具包；全部测试真实被收集；文档无两套现行目标；发布证据完整。

**回滚**：按合并顺序 revert；不使用 reset/restore 旧 stash 覆盖其他分支工作。运行期间的用户数据库副作用按功能自身的恢复约束处理。
