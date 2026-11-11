# 字段展示扩展与时间格式化实现方案

状态：设计提案，未实现。日期：2026-11-11。

关联：[PRD](issue-32-value-presentation-prd.md) · [Issue #32](https://github.com/flyxl/datazen/issues/32)

本文为用户要求保存的设计快照，放在过程文档目录，不代表已交付架构。源码核查基于分支 `chore/e2e-parallel-and-screenshots`，HEAD `2a6363f641c30b412af0640a01ec7dc0c4644fb1`，不是对 main 最新状态的声明。

## 1. 已核实的实现基础

- [formatters.ts](../../../packages/frontend/src/lib/formatters.ts) 的 `formatTimestamp` 保留无时区文本，带时区值经 Date 归一化为 UTC。
- [CellRenderer.tsx](../../../packages/frontend/src/components/DataTable/CellRenderer.tsx) 的数字分支先直接返回；仅改 datetime 分支无法支持 BIGINT。
- [DetailPanel.tsx](../../../packages/frontend/src/components/DataTable/DetailPanel.tsx) 的 `FieldRow` 与 `InlineFieldEditor` 有独立展示路径。
- [ContentViewDrawers.tsx](../../../packages/frontend/src/windows/connection/ContentViewDrawers.tsx) 负责 aside 的行、列和来源接线。
- [extensionPoints.ts](../../../packages/extension-points/src/extensionPoints.ts) 的 `ExtensionRegistry` 每个 EP ID 只保存一个实现，后注册覆盖前实现。
- [lifecycle.ts](../../../packages/extension-points/src/lifecycle.ts) 的 `ExtensionContext` 不提供配置读取能力，只含扩展身份和资源清理列表。
- [SqlEditor.tsx](../../../packages/frontend/src/components/sql-editor/SqlEditor.tsx) 已采用宿主读取设置、传入扩展工厂的模式。
- [SettingsContent.tsx](../../../packages/frontend/src/windows/settings/SettingsContent.tsx) 的设置贡献目前来自 SQL 编辑器 EP，值存于 `driverSettings`。
- [settingsStore.ts](../../../packages/frontend/src/stores/settingsStore.ts) 保存后发布跨窗口设置事件；共享设置与桌面 extra 的存储边界不能混用。

基础格式化单测曾在本次分析中运行，15 项通过；这不验证本文拟新增功能。

## 2. 分层与命名

通用展示能力命名为 `data.valuePresentation`；首个贡献 ID 为 `datazen.timestamp`。二者均为拟新增标识，不宣称已有仓库级命名标准。

扩展点采用「领域.能力」，贡献 ID 采用「提供方.展示器」。未来可加入其他提供方的 URL、图片贡献，但不在首版发布未消费的接口。

职责划分：

- 宿主：来源身份、配置 IO、作用域、跨窗口同步、贡献集合、展示计划缓存、渲染与编辑边界。
- 时间贡献：配置验证、纯迁移、规则解释、列适用性、时间转换与设置表单内容。
- 驱动：数据库类型语义，特别是伪时间类型与方言差异。
- 内核设置用例与仓储：可持久化配置及其用户作用域；HTTP/Tauri 边界仅做接线。

Wapp 不介入主表格，不放宽 iframe 或桥权限。

## 3. 多贡献注册

不将多个展示器直接注册为现有单实现 EP 的同名实现。新增独立的 `ValuePresentationRegistry`，由宿主持有，并通过 extension-points 包公开贡献契约与注册帮助函数。

```typescript
interface ValuePresentationRegistry {
  register(contribution: ValuePresentationContribution): () => void;
  get(id: string): ValuePresentationContribution | undefined;
  list(): readonly ValuePresentationContribution[];
  subscribe(listener: () => void): () => void;
}
```

`data.valuePresentation` 表示这类集合贡献能力；不同时维护一套单实现注册与一套集合注册作为两个真源。

- 重复贡献 ID 显式拒绝，不覆盖已有实现。
- 卸载句柄绑定注册实例，不能移除之后重新注册的新实例。
- 注册帮助函数将释放句柄加入 `ExtensionContext.subscriptions`。
- 注册、卸载触发订阅，增加宿主维护的贡献版本，令所有相关计划失效。
- 贡献顺序不决定谁抢占字段；由配置指定 rendererId，原生时间默认路由至 `datazen.timestamp`。
- 贡献缺失时保留配置，使用默认展示，配置面板说明缺失原因。
- 激活失败需释放本轮已注册资源；核查并补齐现有加载器失败清理，避免半激活贡献残留。

## 4. 首版公共契约

以下是拟新增的接口轮廓，落地时拆分配置 schema 与运行时类型，并经严格 TypeScript 检查。首版只开放 text 输出，不提前接受 link/image 或任意 ReactNode。

```typescript
type PresentationValue =
  | null | undefined | string | number | boolean | bigint
  | readonly unknown[] | Readonly<Record<string, unknown>>;

interface ColumnSource {
  connectionId: string;
  database?: string;
  namespace?: string;
  table: string;
  column: string;
}

interface PresentationColumnContext {
  columnName: string;
  databaseTypeName?: string;
  semanticKind:
    | 'instant' | 'local-datetime' | 'date' | 'time'
    | 'number' | 'text' | 'other';
  source?: Readonly<ColumnSource>;
}

interface PresentationEnvironment {
  locale: string;
  localTimeZone: string;
}

type PresentationResult =
  | { kind: 'text'; text: string; interpretation?: string }
  | { kind: 'unchanged'; reason?: string };

interface PreparedPresentation {
  present(value: PresentationValue): PresentationResult;
}

interface ValuePresentationContribution {
  id: string;
  configVersion: number;
  // JSON 输入在边界验证；未知版本不覆盖旧配置。
  validateConfig(input: unknown): ConfigValidationResult;
  prepare(input: PresentationPrepareInput): PreparedPresentation;
}
```

`ConfigValidationResult` 定义为成功配置 / 路径化验证问题的判别联合，`PresentationPrepareInput` 包含已验证的版本化配置、列覆盖、列上下文和环境快照；这两个类型在实现时完整定义，不使用 any 或以断言跳过验证。

配置与公共契约必须是 JSON 可序列化结构；PreparedPresentation 中函数仅存在于进程内，不持久化。值输入允许 bigint 不代表现有 transport 需要新增 bigint 编码，优先消费原有整数字符串。

设置 UI 贡献可以返回 React 内容，但属于单独配置面板契约；值输出保持数据化，由宿主负责 cell/aside 布局。通用配置外壳提供 value、错误、提交与取消，具体时间表单解释时间参数。首版直接接入展示贡献设置入口，不重构所有 SQL 编辑器设置贡献。

## 5. 配置模型与规则

宿主新增版本化 `valuePresentation` 用户偏好。配置按 rendererId 命名空间保存，时间命名空间包含 enabled、默认格式、规则；表列覆盖单独组织，避免把表定位信息混入格式选项。

```typescript
type ColumnPresentationOverride =
  | { mode: 'inherit' }
  | { mode: 'raw' }
  | { mode: 'custom'; rendererId: string; options: unknown };

type TimeZoneSpec =
  | { kind: 'utc' }
  | { kind: 'local' }
  | { kind: 'iana'; name: string };

type TimestampInterpretation =
  | { kind: 'native' }
  | { kind: 'unix'; unit: 'seconds' | 'milliseconds' }
  | { kind: 'text' };

interface TimestampOptions {
  interpretation: TimestampInterpretation;
  format: 'iso' | 'iso-tz' | 'locale';
  displayTimeZone: TimeZoneSpec;
  sourceTimeZone?: TimeZoneSpec;
}
```

源时区只出现一处，禁止同时在 interpretation 与顶层重复存储。`native` 不把数字自动解释成 Unix；Unix 必须显式指定单位。无时区文本与原生 local-datetime 可使用 sourceTimeZone；带偏移值以自身时刻为准，不用 sourceTimeZone 重解释。

规则数组顺序是唯一优先级，不再保存容易矛盾的 priority 数值。规则内名称条件 AND，首条命中后冻结整列解释。pattern 保存 source 与受限 flags，不持久化 RegExp；缺失来源不能满足表级条件。

正则安全不只依靠长度限制：实现应采用经验证的线性时间引擎或明确限制为可保证复杂度的表达式子集；不得在渲染线程执行任意用户正则。复杂或不支持的表达式在保存时拒绝，规则数量与输入长度同时设限。

## 6. 配置加载、持久化与同步

EP 激活只注册能力，不读取 Store、文件或接口。宿主负责加载配置，验证后以不可变快照传入 prepare。

全局和表覆盖配置均由宿主使用同一用户偏好存储端口管理。Desktop 复用设置仓储；Web 复用用户级设置用例。补齐 backend-client、Rust 共享设置 DTO、默认值与宿主映射，不能只加桌面 extra 或借用 driverSettings/wappSettings。

表覆盖键使用结构化字段：当前用户/组织作用域 + connectionId + database + namespace + table + column。存储层负责用户/组织作用域，不能相信客户端传入的任意用户 ID。保留驱动返回的标识符大小写语义，不自行统一小写；缺省字段与空字符串不能无意碰撞。

加载状态：

1. 初次加载中：cell 与 aside 使用同一个默认快照。
2. 全局与当前表配置全部就绪：一次发布新快照，生成计划。
3. 后续刷新：继续使用最后一份有效快照，成功后整体替换。
4. 切换上下文：以面板来源和请求代次校验异步响应，旧表响应不得覆盖新表。
5. 保存失败：保持已生效快照，编辑面板保留草稿与错误。

配置写操作用路径化更新/版本条件更新避免跨窗口覆盖整个旧快照；复用现有机制前验证其并发行为，不把广播当并发控制。成功保存后发布作用域与配置修订通知，接收端重新读取权威配置；同窗口从权威响应更新。Web 跨标签页同步采用现有事件端口可用实现或受作用域约束的广播适配，不承诺跨设备实时同步。

历史配置缺字段默认关闭。未知配置版本或暂时缺失贡献时保留原文，不用默认值写回覆盖。纯迁移由贡献声明，持久化由宿主执行，迁移失败可重试。

## 7. 共享展示管线

```text
设置仓储 → 宿主配置快照 ──────────┐
驱动类型 / 可靠列来源 ───────────┤
展示贡献集合与环境快照 ──────────┤
                                ↓
                     面板列展示计划（prepare）
                                ↓
原值 ───────────────→ 统一 presentValue
                          ↙           ↘
                    GridCell         DetailPanel
```

在面板层引入共享 controller/hook，列计划通过 props/context 注入 DataTable 和 DetailPanel，组件不直接读 Store 或 IPC。aside 即使位于另一组件子树，也引用当前来源面板的同一个 controller。

修改 CellRenderer：编辑分支继续接收原值；NULL 优先；之后尝试展示计划，再走原有 bool/number/datetime/json 等默认分支。因此 BIGINT 在默认数字分支返回前可以被显式时间计划处理。

修改 DetailPanel：非编辑态使用同一展示结果；InlineFieldEditor 进入编辑时从原值初始化，取消恢复展示，提交仍使用既有类型校验。不把格式化文本写进行数据或 pending changes。草稿行和查询结果网格接入同一逻辑。

计划缓存键至少包含来源身份、列元数据版本、配置修订、贡献修订、locale、localTimeZone。规则与 Intl 格式器在 prepare 阶段创建；不逐格编译、不在 render 调 IPC。系统时区变化应通过环境快照更新触发失效，可在窗口恢复前台时重新检查。

扩展异常由宿主统一隔离；发生系统性异常时按贡献修订熔断并令两处回退，避免只有某个组件失败。普通非法值返回 unchanged，不作为扩展异常。日志记录贡献与原因码，不记录原始行值或连接凭据。

## 8. 时间解析算法与精度

1. 使用规范化类型判定原生时间，数据库特殊类型经 DatabaseTypeMeta/驱动声明解决，不在宿主硬编码驱动名称。
2. Unix 字符串先验证整数语法，再用 BigInt 精确解析；number 必须 Number.isSafeInteger。
3. 秒乘 1000 使用精确整数运算，检查支持的日期范围后才转用于格式器的数值。0 与负数正常处理。
4. 带时区文本严格解析时刻与小数秒；无时区文本不交给隐式本地解析的 new Date(string)。
5. 使用支持 IANA 与显式 DST 歧义策略的时间实现。实施首阶段先核查现有依赖和 Desktop/WebView 兼容性，再选库；不能假定原生 Temporal 普遍可用。此项是格式器交付前置条件。
6. 小数秒与时刻转换分开处理，保留原生微秒/纳秒精度；若依赖无法保证则返回 unchanged，不静默截断。
7. iso 输出 UTC；iso-tz 使用目标时刻对应的实际偏移；locale 固定传入语言和时区，不能依赖机器隐式默认值。locale 格式中精度需显式保留，无法实现时回退。
8. 无时区源未配置、DST 歧义、范围错误使用稳定原因码；预览和实际展示调用同一解析实现。

## 9. 文件改动边界

拟新增模块：

- `packages/extension-points/src/valuePresentation/`：贡献契约、集合注册、清理与订阅测试。
- `packages/frontend/src/lib/valuePresentation/`：配置快照适配、列计划、默认回退与统一展示。
- `packages/frontend/src/hooks/useValuePresentation.ts`：宿主状态接线。
- `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`：统一列配置入口。
- 时间实现应落在独立 EP 包；包路径和构建装配按仓库既有特权扩展发布方式确定，不伪装成 Wapp，不手改 generated 文件。

需修改的既有位置：CellRenderer、GridCell/草稿行传参、DetailPanel、ContentViewDrawers、表头菜单接线、表与查询结果面板、TableDataSettingsSection、settingsStore、设置 DTO 与持久化适配、相关 i18n。

需要从核查后的当前代码确定最小改动范围，本文不要求机械修改每个中间组件。不扩展 Driver Command 获取展示配置，不让 command/HTTP handler 直接访问存储 internals。

## 10. 分阶段实现与验证

### A：贡献与展示基础

完成集合注册、清理、缺失回退、配置契约；抽取共享管线并保证禁用时兼容。验证重复 ID、旧卸载句柄、激活失败清理、cell/aside 默认一致和编辑原值。

### B：时间与列配置

实现格式器、全局开关、两处列配置入口、Unix 解释与持久化。覆盖时区跨日、正负偏移、DST、NULL、0、负时间、不安全整数、数字字符串、越界、非法文本、无时区值与高精度。

### C：规则与双端恢复

完成规则编辑、稳定顺序、正则约束、跨窗口同步、来源隔离、异步过期响应与查询结果来源降级。

完整 E2E：开启 → 表头设 Unix 毫秒 → 检查 cell 与 aside → aside 改时区 → 编辑仍为整数 → 保存/重开 → 切同名其他表 → 清除覆盖跟随规则 → 调整规则顺序 → 停用/重新启用扩展。额外覆盖分屏与查询结果切换。

通用旅程放 e2e；驱动特殊类型测试放对应驱动目录，例 SQL Server timestamp/rowversion 必须保持非时间。定位使用 data-*，不用视口坐标。

验证按改动执行相关 Vitest、pnpm typecheck:tsc、相关 Rust 设置用例与宿主测试；涉及契约执行边界门禁。E2E 使用 pnpm e2e 或规定的 webdriver 构建流程。环境缺失、超时和资源不足单独报告，不能报为通过。

## 11. 后续 URL / 图片扩展

注册、配置命名空间、原值/展示分离与两处消费者复用是本轮扩展基础。未来新增展示器时按需扩展输出协议与版本，不允许未知 kind 被当前宿主误处理。

链接需要宿主负责协议校验与打开行为；图片需要网络隐私、鉴权、资源大小、加载失败与预览策略。届时再定义 link/image payload，而不是现在开放任意 HTML、ReactNode 或未经限制的资源请求。

## 12. 文档落地

完成实现后将使用入口、时间语义和例子写入 [数据浏览指南](../../features/data-browsing-guide.zh-CN.md)，将共享展示管线与原值不变量写入 [DataTable 架构](../../architecture/frontend/data-table.md)，按实际交付更新扩展开发说明。本文件保留设计历史，不冒充持续维护的实现事实。
