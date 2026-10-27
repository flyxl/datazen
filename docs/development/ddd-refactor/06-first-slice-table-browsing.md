# 第一条完整示范：连接配置到表浏览（原 §9）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §9 的全部内容；文档权威与阅读路线见总纲 §1。

## 9.1 修改前先追踪当前链路

当前核对入口：

- `src/commands/database.ts` 的表数据请求与 `src/platform/tauriBackendTransport.ts` 的 transport 映射。
- `src-tauri/src/commands/data.rs` 的 `get_table_data_at_target` 与 `commands/table_target.rs` 的 `read` / `execute`。
- `packages/feature-core/src/services/query_executor/` 的旧查询服务及 `commands/schema.rs` 的旧 `get_table_data`。
- `modules/src/execution/application/` 的执行、事务与结果实现。
- `src/stores/tableDataStore.ts` 的分页、结果合并和前端状态处理。

旧链路不等于可以直接删除的死代码。实施前搜索全部前端、MCP、Workflow、测试和注册入口，确认是否存在只提供 session 的调用方。

## 9.2 新链路

Controller 将既有 wire 请求映射为 `table_browsing::application` 的读页请求，后续顺序为：

1. 校验 page size、filter/sort 结构及必须提供的目标信息。
2. execution/connection 应用契约读取可信配置并校验权限、revision 和驱动能力；不信任客户端自报 driver type。
3. schema 应用服务取得表结构，按当前能力形成列、主键和排序信息。
4. table_browsing domain 计算分页、keyset 可用性、count 计划和拒绝原因。
5. 使用 Driver 契约生成目标命令，交给 execution 应用服务执行；表浏览不直接调用具体 Driver 的物理 query。
6. 通过 execution 等待与结果读取能力取得数据，转换为现有 TableDataResult。
7. Controller 映射错误；前端继续消费同一 DTO。

模块内部的 `TableReadRequest` 与写入上下文解耦，共享目标由 schema 的公开契约提供。现有 IPC 请求字段不能因此改名或删除，适配器负责转换。

## 9.3 两种旧入口如何收敛

- `get_table_data_at_target` 和旧 `get_table_data` 最终进入同一套读页业务实现。
- target 调用保留独立 lease；session 调用保留已验证 handle 与事务语义。
- 旧入口若只有 dbSessionId，必须通过受授权的会话解析能力取得目标和有效 handle，不能猜测 connectionId 或填默认 database。
- 可以保留不同的入口适配 DTO，不能保留两份分页/count 算法。
- 旧入口没有生产调用方后才删除注册；仍在兼容期的 IPC 可以保留，但只负责映射并委托。

## 9.4 已落地的命令族（截至 2026-10-25）
- 顺序建议：连接管理（契约最全、测试最多）→ schema-diff → sync → transfer → ai/workflow。
- 每个 feature：commands 里抽出用例服务到 `application`，宿主只保留 IPC 壳。
- **已落地的第一条命令族（2026-10-23，命名视图）**：`ApplicationServices` 增加
  `named_views: Arc<dyn NamedViewRepository>`，`commands/named_views.rs` 的三处
  `state.store.as_ref()` 全部改走聚合，**宿主 store 在该文件的生产半边出现 0 次**
  （新增断言 `the_command_shell_reads_the_aggregate_not_the_host_store` 钉住）。
  连带的两条结构性事实：
  - `application` 现在**依赖** `datazen-feature-core`。这不是回摆：P0 断环 + P2 收官之后
    `APP → feature-core → kernel/platform-api` 是正确方向，而「用例层不得依赖领域包」
    那条 manifest 禁令的理由（`APP → 领域包 → RT → APP` 的 Cargo 环）已经不存在。
    该禁令改写成新断言 `the_crate_may_depend_on_the_shared_feature_layer_but_not_on_the_runtime`
    —— 保留 `datazen-modules` / `tauri` 两条真禁令，环的判定交给门禁按**当前**图求值。
  - `scripts/check-ddd-layers.mjs` 的镜像不变量不再硬编码「消费者一定依赖 application」，
    而是**按当前图求值**（消费者此刻确实依赖 application 时才判环），并补了双向自证
    （仍互依赖的合成图必须报警、单向图必须不报警）。
- **第二条命令族（2026-10-23，仪表盘）**：聚合增加
  `dashboard: Arc<dyn DashboardRepository>` 与 `dashboard_runs: Arc<dyn DashboardRunRepository>`
  （两个端口都由 `packages/feature-core/src/dashboard/` 定义，桌面实现都是 `AppDb` ⇒
  端口定义在领域包、实现在宿主，聚合只拿 trait 对象）。`commands/dashboard.rs` 的 14 处
  `state.store.app_db()` 中 **8 处归位聚合**（仓储 6 + 运行历史 2），残留 6 处**有名字**的
  形态专属用途（组件创建 3、导入导出 2、手工执行 1），由断言
  `the_command_shell_reads_the_aggregate_and_names_the_host_residue` 钉住。
  全仓生产半边计数 101 → 93。
- **第三条命令族（2026-10-23，连接目录）**：这一族与上两族的差别是**端口住在哪里**。
  连接配置的形状 `ConnectionConfig` 住在 `datazen-driver-api`（每个形态都已链接的那一份，
  且是 IPC 载荷与前端表单的唯一形状），所以端口没有进领域包，而是进
  `datazen_modules::platform_api::ports::connection_catalog`——`platform-api` 本来就依赖 driver-api
  （`TunnelRepository` 同理），端口里出现 `ConnectionConfig` 不构成新依赖。
  `ApplicationServices` 增加 `connections: Arc<dyn ConnectionCatalog>`，桌面实现是
  `DesktopRepository`；`commands/connection.rs` 与 `commands/connection_import/ipc.rs`
  生产半边 `state.store` 归 0（断言 `the_command_shell_reads_the_aggregate_not_the_host_store`
  钉住 8 处取聚合、12 处端口调用），全仓生产半边 93 → 84。
  两条**当时没被写进表格的隐账**在本族被结清：`duplicate_connection_impl` 与
  `reorder_connections` 的 `state.store` 是跨行书写的，逐行 grep 数不到；真实处数是 11 + 2。
   另外把「端口调用 → 命令错误」的映射收口成 `commands/port_call.rs::port_cmd`
   （`tunnel.rs` 的同名助手改为转调它），避免每个命令族各抄一份 `PortError` 映射。
- **第四条命令族（2026-10-23，设置与分组）**：分组读写复用连接目录聚合的
  `ConnectionCatalog::groups/save_groups`；完整 `AppSettings` 读写复用设置仓储的 opaque
  document 方法，typed 的 `SharedSettings` 仍由同一仓储提供。这样主题、键位、执行策略等
  跨形态字段继续走 `SharedSettings`，而路径、onboarding 等桌面字段不被复制进中立 DTO。
  `commands/config.rs` 的生产半边 `state.store` **12 → 4**，`app_archive.rs` **3 → 2**，
  `window.rs` **2 → 0**；保留的六处仅为 §5.1.3 的 `data_dir()`，另有一处钥匙导出仍待
   宿主密钥门面。全仓生产半边 **84 → 73**。`shared_settings_commands_read_the_aggregate_not_the_host_store`
   同时钉住设置/分组不得直读 store 与四条设置读写路径必须经过聚合。
- **第五条命令族（Schema Diff profile 与 owner 配置读取）**：Schema Diff 领域包新增
  `SchemaDiffProfileRepository`，端口与 `SchemaDiffProfile` 词汇同住
  `modules/src/migration/schema_diff`；`ApplicationServices` 持有该端口，桌面实现仍是同一个加密
  `DesktopRepository`。`profile_commands.rs` 的 profile 增删查、部署前的持久化只读检查、
  Job capability / target database / target schema 的 owner 配置读取统一经
  `ApplicationServices.connections`，`commands/schema_diff/` 生产半边的 `state.store`
  直读由 **4 → 0**（以当前命令源码中 `state.store` 直接引用计数），本次全仓同口径
  计数 **79 → 75**。本族额外保留作用域测试：非桌面组织的读取按空集合处理，写入按
  NotFound 失败关闭；领域端口与桌面实现的 round-trip 由
  `schema_diff_profiles` 仓储测试覆盖。
- **迁移历史 revision 校验也已收口**：既有 `platform-api::MigrationProfileReader` 补齐
  `updated_at` revision 投影并由桌面 `DesktopRepository` 实现，`history.rs` 的三类
  profile 查找改走 `ApplicationServices.migration_profiles`；命令壳不再点名
  `get_sync_profiles` / `get_transfer_profiles` / `get_schema_diff_profiles`，并保留
  「缺 profile / revision 不一致」的原有阻断语义。
- **验收**：该 feature 的 `commands` 文件行数下降 ≥50%；`grep state.store.` 在该 feature 目录降为 0；IPC 签名与返回类型不变（用既有前端类型做回归）。
- **回滚**：按 feature 分批，每批独立 PR。
- **风险**：双轨期重复逻辑 → 冻结规则：新代码必须进用例层，旧代码只减不增。

## 9.5 本轮不能悄悄改变的语义

- 无主键、复合主键、NULL 排序、keyset 拒绝与 offset fallback。
- `require_keyset`、`after_key`、cursor depth、默认排序和翻页参数。
- count 的复用、不可用、精确、估算不是同一状态；失败不能伪装成 0。
- 手动事务会话目标变化必须拒绝，不在后台换连接继续执行。
- schema / rows / count 目前分次执行，收进同一个 Service **不自动提供同一 snapshot**。增强一致性需要 Driver 能力与单独行为设计。
- 前端 pending changes、count drift、缓存失效和错误后的下一步操作仍可用。

验收必须包含连续旅程：打开表 → 筛选 → 排序 → 翻页 → 修改配置/会话上下文 → 再读被拒绝 → 刷新后恢复，不能只测一条最终合法查询。

## 9.6 这条切片的完成标准

宿主表浏览命令不再访问 Store / AppDb / driver registry / runtime internals；两个入口共享同一应用流程；配置与资源状态只有一份；领域规则独立可测；桌面运行旅程通过。Web 集成验证随 D7 延后（2026-10-26 决策移出本次范围），届时对同一公共用例执行，不能重新写一份 Web 表读取算法。
