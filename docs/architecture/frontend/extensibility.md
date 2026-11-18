# 扩展性 — 数据库类型与驱动系统

> [返回架构总览](../README.md)

### 1.1 设计原则

1. **注册表驱动**：`src/lib/databaseTypes.ts` 的 `DB_REGISTRY` 是所有 DB 类型行为的单一数据源（驱动条目经 codegen 的 `DRIVER_DB_ENTRIES` 在此合并，见 1.4）
2. **路由层 if-else**：仅在组件选择边界使用 if-else，不在组件内部散布方言/类型分支
3. **策略模式**：SQL 方言逻辑集中在 `src/lib/sqlDialects/`，连接视图集中在 `src/lib/connectionViews/`

### 1.2 核心模块

```
src/lib/
├── databaseTypes.ts          # DB_REGISTRY + 行为标志 + normalizeRedisDatabaseField
├── sqlDialects/              # DDL / 索引 / 备份 方言策略
│   ├── postgresql.ts, mysql.ts, sqlite.ts
│   └── index.ts              # getSqlDialect(dbType)
└── connectionViews/          # 连接窗口视图注册表
    └── index.ts              # CONNECTION_VIEWS + getConnectionView(mode)

src/components/connection/    # 共享连接表单
├── useConnectionForm.ts      # 表单状态 + Kiwi 登录 + draft 构建
├── ConnectionFormBody.tsx    # 按 connectionForm 路由到 Fields 组件
├── KiwiConnectionFields.tsx
├── FileConnectionFields.tsx
├── IndexConnectionFields.tsx
└── StandardConnectionFields.tsx

src/windows/connection/
├── ConnectionPage.tsx        # 统一主工作区壳：导航 + 连接 Tab + Workflow/Dashboard
├── ConnectionNavigatorTree.tsx # 左侧连接/Schema 导航
├── SqlConnectionView.tsx       # SQL 连接 UI
├── DocumentConnectionView.tsx  # 文档型连接 UI
└── schema-tree/
    ├── schemaTreeCategories.ts  # 分类定义与驱动 → 分类映射
    ├── schemaTreeDrag.ts        # 行拖拽编排
    └── useExpandedDbCacheRefresh.ts # 展开库时的表缓存刷新
```

### 1.3 DB_REGISTRY 行为标志

| 字段                | 用途                                                                                                |
| ------------------- | --------------------------------------------------------------------------------------------------- |
| `connectionView`    | 路由到 `CONNECTION_VIEWS`（sql / keyvalue / document）                                              |
| `connectionForm`    | 路由到连接表单 Fields 组件（standard / kiwi / file / index）                                        |
| `sqlDialect`        | 路由到 `sqlDialects/` 策略                                                                          |
| `hasMultiDatabase`  | 驱动**能力**标志。连接树通过 `navigator/utils.ts` 的 `shouldUseMultiDatabaseTree` 切换单库/多库渲染 |
| `databaseFieldType` | `name` / `path` / `index` / `domain`（Kiwi 实例域名）                                               |
| `defaultPageSize`   | 覆盖默认分页（如 Kiwi 1000 行）                                                                     |
| `supportsBackup`    | BackupWindow 过滤 + 方言备份选项                                                                    |

### 1.4 添加新 DB 类型检查清单

新 DB 类型一律以**驱动包**（`crates/drivers/<id>/` 或独立 git 仓库）形式添加，不再手改宿主注册表：

1. `drivers-registry.json`（或 gitignored 的 `.drivers-dev.json`）— 注册驱动（`source: "path"` / `"git"`）
2. 驱动 Rust crate — `datazen-driver-api` trait 实现 + `inventory` 注册
3. 驱动前端入口 `ui/meta.ts`（或 `ui/shared/meta.ts`）— `DatabaseTypeMeta` 条目（含 `connectionForm` / `connectionView`）；`DatabaseType` 联合与 `DB_REGISTRY` 条目由 `scripts/resolve-drivers.mjs` codegen 的 `src/extensions/generated.ts`（`DRIVER_DB_ENTRIES`）自动合并，**禁止**在 `src/types/index.ts` / `src/lib/databaseTypes.ts` 手工添加
4. （可选）驱动 `ui/` — 连接表单 Fields、连接视图、Schema 树、SQL 方言策略、设置分区
5. （可选）驱动 `locales/` — 前端词条（契约见 [驱动 ↔ 宿主解耦契约](../../development/driver-api-dependency-boundary.md) 2.4）

宿主内置的表单/视图路由组件（`components/connection/`、`connectionViews/`、`schema-tree/`）仍按下表标志分发。前端 import 面与能力注入规则见 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md) 与解耦契约 Part 2。

### 1.5 主题包 vs 数据库驱动

主题包与数据库驱动**不共享**安装路径或注册表：

|      | 数据库驱动                                                            | 主题包                                                                                      |
| ---- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 路径 | `crates/drivers/<id>/`（path 已提交；git 构建时 clone，gitignored） | Wapp 包内 `contributes.themes[]` 声明（运行时安装到 `{appData}/wapps/{publisher}.{name}/`） |
| 注册 | `drivers-registry.json` + `DB_REGISTRY`                               | 文件系统 + `manifest.json`                                                                  |
| 扩展 | Rust crate + 前端 meta                                                | CSS / JSON / SVG\|PNG\|WebP / 字体（无 JS）                                                 |

驱动在无主题包（`packId: null`）下正常工作；主题包可覆盖 `db.<type>` 图标与 `--dt-*`（含 `--dt-binary`）DataTable 单元格色，但不改变驱动协议。Redis 深度 UI 位于 `crates/drivers/redis/ui/`（非 Host `src/windows/connection/`）。

### 1.6 SQL 编辑器特权扩展点（Host ↔ Pro）

SQL 编辑器的增强能力通过 `sqlEditorEnhancedEP`（`packages/extension-points/src/sqlEditorEnhancedEP.ts`）注入。宿主是唯一的 Compartment 与键位所有者：扩展只贡献钩子实现，**不能**自行安装 Compartment、不能重排宿主键位优先级。

**两级空实现，Community 版因此可以零 Pro 代码运行：**

1. EP 自身为每个钩子提供了返回空值的默认实现（`sqlEditorEnhancedEP.ts:170-181`）；
2. 宿主调用点再用 `?? []` 兜一层（`src/components/sql-editor/editorExtensions.ts:379`、`:619`、`:667`、`:696`、`:724`）。

**能力归属（已核对代码）：**

| 能力 | 归属 | 落点 |
| ---- | ---- | ---- |
| 补全源（JOIN / 限定列）、语句装饰、签名帮助 | Pro | `createJoinCompletionSource` / `createColumnCompletionSource` / `createStatementDecorations` / `createSignatureHelpExtensions` |
| 意图（Alt+Enter）、悬停跳转、Linter | Pro | `createIntentionExtensions` / `createHoverExtensions` / `createLinterExtensions` |
| Code Folding | Pro | `createFoldExtensions`（`sqlEditorEnhancedEP.ts:140`），Compartment 仍由宿主持有 |
| 粘贴为 IN、表拖放等粘贴增强 | Pro | `createPasteExtensions` / `createPasteAsInContextMenuItems` |
| 多光标、查询历史、收藏 | 宿主 | `src/components/sql-editor/`、`windows/connection/query/` |

**拖放负载契约是不对称的（列拖放尚不可达）**：V1 负载的 `kind` 在 Pro 消费端声明为 `'table' | 'view' | 'column'`，解析时保留 `column`（`packages/pro-extensions/sql-editor-pro/src/paste/dropCaret.ts`，附 `validates column payload structure` 单测）；但宿主生产端 `src/windows/connection/schema-tree/schemaTreeDrag.ts:21,47` 的联合类型只有 `'table' | 'view'`，也从不把列节点构造为可拖拽源。因此「从结构树拖一列进编辑器插入列限定名」这条链路**消费端就绪、生产端缺失**，功能整体不可达（无崩溃、无错误结果，属半实现特性）。补齐点在宿主：为列节点加拖拽源并扩展负载联合类型，Pro 侧无需改动。缺陷的现象、影响面与逐段链路实测见 [SE-PROD-011](../../bugs/SE-PROD-011-column-drag-drop-unreachable.md)。

**测试落点（重要）：** 宿主 `e2e/` 构建的 Community 版里，上述 Pro 钩子全部走空实现，因此在 `e2e/specs/` 里断言 Pro 行为必然失败——不是用例写错，是被测代码根本不在宿主里。**凡由 EP 钩子实现的能力，其 E2E 必须写在 Pro 包内**（`packages/pro-extensions/sql-editor-pro/e2e/specs/`），与「驱动测试写在驱动 crate 内」是同一条纪律。这条规则的直接后果：改 Pro 代码后必须先重新 stage（`resolve-pro` 对已存在的 `builtin-ep` 目录会跳过重新打包），否则测的是上一次的产物。

Pro 包是**独立 git 仓库**（gitignored），需独立 commit / pull / push；宿主 PR 不会带上它的改动。

### 1.7 值呈现集合贡献（`data.valuePresentation`）

字段展示能力通过**集合注册**接入宿主，而不是单实现扩展点：多个展示器可以同时注册，由配置指定每列用哪一个。首版只交付时间展示器 `datazen.timestamp`；URL、图片展示器不在本轮范围，也不要提前开放 link/image 或任意 HTML/ReactNode 输出。

**契约**（`packages/extension-points/src/valuePresentation/`，`contracts.ts` / `registry.ts` / `lifecycle.ts`）：

| 内容 | 落点 |
| --- | --- |
| 贡献契约 | `ValuePresentationContribution`：`id`、`configVersion: number`、`validateConfig(input: unknown): ConfigValidationResult`、`prepare(input: PresentationPrepareInput): PreparedPresentation` |
| 输出协议 | 首版只开放 text 输出：`PresentationResult = { kind: 'text'; text; interpretation? } \| { kind: 'unchanged'; reason? }`；`PreparedPresentation.present(value)` 是纯函数，不调 IPC、不读 Store |
| 集合注册 | `ValuePresentationRegistry`（`register` / `get` / `list` / `subscribe`）+ 全局单例 `valuePresentationRegistry`；**重复贡献 ID 显式拒绝**，不覆盖已有实现；卸载句柄绑定注册实例，不能移除之后重新注册的新实例 |
| 注册帮助函数 | `registerValuePresentation(context, contribution, registry?)`：把释放句柄记入 `ExtensionContext.subscriptions`，与 `registerExtensionPoint` 同款语义——扩展卸载与激活失败清理时按 LIFO 自动释放 |

**与单实现 EP 的区别**：`ExtensionRegistry` 每个 EP ID 只保存一个实现、后注册覆盖前实现；`ValuePresentationRegistry` 是按贡献 ID 索引的集合，重复 ID 抛错。能力名 `data.valuePresentation` 目前**没有代码级常量声明**（`contracts.ts:133` 注明由宿主侧声明），不要在新代码里再散落该字符串。

**首个贡献 `datazen.timestamp` 是核心内置的，不是独立扩展包**：它实现在 `packages/frontend/src/lib/valuePresentation/timestampContribution.ts`，随应用启动由 `registerBundledValuePresentationContributions()`（`registerBundledContributions.ts`）注册进全局单例（`startSharedApp.tsx` 调用，幂等、永不卸载）。设计提案里「时间实现应落在独立 EP 包」的说法是提案快照，**实际交付不在此列**——不要按它找包或写新包。第三方展示器仍可经 `registerValuePresentation` 从扩展模块注册；宿主是唯一的配置持有方，贡献只负责验证、准备与呈现。

**职责划分（已交付）**：

- 宿主：来源身份、配置 IO、作用域、跨窗口同步、贡献集合、展示计划缓存、渲染与编辑边界。配置加载后验证，以不可变快照传入 `prepare`；全局配置体有效性由贡献 `prepare` 内部校验，custom 覆盖的 options 在宿主边界验证（两者分工不同，见 [DataTable 架构](data-table.md) §14.5）。
- 时间贡献：配置验证、纯迁移、规则解释、列适用性、时间转换与设置表单内容。配置与公共契约必须是 JSON 可序列化结构；`PreparedPresentation` 中的函数仅存在于进程内，不持久化；`pattern` 只保存 source 与受限 flags，不持久化 `RegExp`。
- 驱动：数据库类型语义，特别是伪时间类型与方言差异；宿主不硬编码驱动名称。

**停用回退**：扩展停用（`unregister`）后，宿主计划对该渲染器返回 `missing-renderer`，网格与 aside 同时回退默认展示，已保存配置保留；重新注册后恢复，无旧缓存残留。停用是 per-JS-context 的（每个窗口一份注册中心实例），跨窗口同步的范围是配置，不是注册表状态。
