# 驱动 ↔ 宿主解耦契约（Driver ↔ Host Dependency Boundaries）

> 本文件是 DataZen 驱动与宿主之间依赖边界的**唯一规范落点**。
> **Part 1** 约束 Rust 层 `packages/driver-api` 公共 API；**Part 2** 约束驱动前端（UI import 面、共享类型、i18n、宿主能力注入桥）。
> 历史名：Driver API Public Dependency Boundary（Part 1 即其原文）。
> 配套实操指南：[独立驱动开发指南（中文）](independent-driver-development.zh-CN.md) / [English](independent-driver-development.en.md)。

---

# Part 1 · Rust Driver API 公共依赖边界

`packages/driver-api` is the stable compile-time contract between DataZen and independent database driver extensions.

The API is published as the MIT-licensed `datazen-driver-api` crate. Its source of truth is the DataZen monorepo; there is no separate Driver API source repository.

## Public API rule

Public Driver API signatures and public fields may use:

- Rust primitives and standard-library types;
- types defined by `datazen-driver-api` itself;
- `serde` traits/attributes where required;
- `serde_json::Value` and other transport-neutral JSON data types.

They must not expose database implementation types.

## Forbidden public dependencies

The following must not appear in public traits, function signatures, public struct fields, enum variants, or public type aliases:

- `sqlx` types such as `Pool`, `Row`, `Transaction`, or database-specific errors;
- `tokio` runtime or synchronization types;
- database-specific crates such as `mongodb`, `redis`, `clickhouse`, `mysql_async`, `tokio-postgres`, and similar libraries;
- HTTP client implementation types such as `reqwest`;
- database pools, rows, transactions, cursors, or implementation-specific error types;
- any other third-party implementation type that an independent driver may reasonably need at a different version.

For example, this is forbidden:

```rust
pub trait DatabaseDriver {
    fn pool(&self) -> sqlx::Pool<sqlx::Postgres>;
}
```

Instead, the Driver API should expose an opaque handle:

```rust
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}
```

The driver owns and manages the real connection pool internally.

## Dependency layering

The intended dependency graph is:

```text
                         DataZen Host
                              │
                    datazen-driver-api
                              │
              ┌───────────────┼───────────────┐
              │               │               │
          Driver A         Driver B        Driver C
              │               │               │
           sqlx 0.7         sqlx 0.8        mongodb
              │               │               │
           private          private         private
```

Different drivers may use different database libraries or different versions of the same library. This is safe because implementation types never cross the API boundary.

## Foundation dependencies

`serde` and `serde_json` are allowed as transport-neutral data dependencies.

`async-trait` is currently used to express asynchronous driver traits. It is part of the Rust API implementation surface, but it is not a database implementation dependency.

`inventory` is intentionally used for compile-time driver registration. DataZen embeds drivers into the application binary rather than loading Rust shared libraries at runtime.

## Versioning

The Cargo crate version and the DataZen Driver protocol version are separate:

- **Crate version** follows Cargo/SemVer compatibility rules for the Rust API.
- **`PROTOCOL_VERSION`** represents DataZen ↔ Driver API protocol compatibility.

Internal dependency changes that do not affect public API types do not require a protocol-version change. Breaking public trait or protocol changes must be evaluated for both versions.

## Review checklist

Before merging a change to `packages/driver-api`:

- [ ] No `sqlx` type appears in public API.
- [ ] No database-specific crate type appears in public API.
- [ ] No `tokio` type appears in public API.
- [ ] Connection pools remain owned by the driver.
- [ ] Rows, cursors, and transactions use API-defined types or opaque handles.
- [ ] Cross-boundary errors use `DriverError` or another API-defined error type.
- [ ] Generic JSON data uses `serde_json` rather than a database-specific document type.
- [ ] New third-party dependencies are checked for accidental public exposure.
- [ ] Crate-version and protocol-version implications are considered.

## Development and publishing

DataZen itself consumes the crate through the workspace path dependency:

```toml
[workspace.dependencies]
datazen-driver-api = { path = "packages/driver-api" }
```

Independent extensions normally consume the published crate:

```toml
[dependencies]
datazen-driver-api = "0.1"
```

When developing an API change before publication, an independent extension can temporarily use a local path dependency pointing at `packages/driver-api`.

---

# Part 2 · 驱动前端与宿主解耦契约

> 适用范围：`packages/drivers/<id>/ui/**`（path 驱动与 git 驱动的前端代码，同一套规则）。
> 心智模型：驱动前端与 Rust 侧对偶——Rust 走 `datazen-driver-api` trait，前端走 `@datazen/driver-sdk` + `@datazen/ui` 两个包；宿主业务代码（`src/**`）对驱动是**不可见**的。

## 2.1 允许与禁止的 import 面

### 2.1.1 允许

驱动前端**只允许** import 以下四类来源：

| 允许来源 | 包名 / 形态 | 提供内容（以各包 `index.ts` 实际导出为准） |
| --- | --- | --- |
| 公共设计系统 | `@datazen/ui`（`packages/ui/src/index.ts`） | 基础组件 `Button` / `Input` / `Select` / `Dialog` / `Tabs` / `Badge` / `Label` / `Slider` / `TemporalValueInput` / `PathInput`；工具 `cn`；i18n 运行时 `t` / `useI18n` / `registerTranslations` / `getRegisteredTranslations`（只读快照，供工具/测试用，见 2.4.1） / `getLocale` / `setLocale`（`setLocale` 的调用约束见 2.4）与类型 `I18nParams` |
| 驱动前端 SDK | `@datazen/driver-sdk`（`packages/driver-sdk/src/index.ts`） | 元数据契约 `DatabaseTypeMeta` / `ConnectionMode`；方言类型与 `BaseTableSqlGenerator`；下沉共享类型（`types/`：`ConnectionFormState`、`KeyEntry` / `KeyScanResult`、`NativeMenuItemDef` / `NativeMenuPredefined`、`ConnectionViewProps` 及配套）；Command IPC 封装 `driverCommands` / `fileCommands`；纯函数 `mergeDriverSettings` / `readBooleanField` / `applySchemaDefaults` / `listBooleanSchemaFields` / `listSchemaPropertyEntries` / `resolveEditorFontFamily` / `HOST_DEFAULT_EDITOR_FONT`；右键菜单 `showNativeContextMenu` / `hideNativeContextMenu` / `normalizeNativeMenuItems` / `nativeEditMenuItems` / `createNativeContextMenuHandler`；注入桥 `bind*` / `useBound*`（见 2.3）；Schema 同步 `syncSchemaTables` / `syncSchemaNamespace` / `registerPathAliases` / `getCachedPathItems` / `cachePathItems` / `subscribeSchemaPathItems` |
| 特权扩展点契约 | `@datazen/extension-points` | **仅 EP 契约类型**（扩展点定义 / 生命周期 / SQL Editor 增强契约）。普通数据库驱动通常不需要 import 它；该包**不导出任何 i18n 能力** |
| npm 依赖 | 驱动仓库自行声明的第三方包 | `react`、`react-dom` 及必要的 UI/工具库（自行承担版本与体积决策） |

裸包名 specifier 的解析方式（三处保持一致，均已存在）：

- 宿主应用构建：根 `tsconfig.json` 的 `paths` 与 `vite.config.ts` 的 `resolve.alias` 将 `@datazen/ui` / `@datazen/driver-sdk` 等映射到 `packages/*/src/index.ts`；
- 驱动单元测试：`vitest.drivers.config.ts` 的 `alias` 提供同样的映射，`pnpm test:unit:drivers` 即可脱离宿主跑驱动 UI 测试；
- 独立驱动仓库（git 驱动）：把宿主仓库的 `packages/ui`、`packages/driver-sdk` 作为本地 path 依赖引入同样的源码。

### 2.1.2 禁止

**禁止任何指向宿主 `src/**` 的 import**（含 `src/hooks`、`src/stores`、`src/lib`、`src/types`、`src/components`、`src/locales` 与 `src/extensions`），无论相对路径嵌套多深。

反例（❌ 均为违规写法，切勿照抄）：

```ts
// ❌ 反例 1：从宿主相对路径 import hook
import { useI18n } from '../../../../../src/hooks/useI18n';
// ❌ 反例 2：从宿主相对路径 import 工具 / 类型
import { cn } from '../../../../../src/lib/cn';
import type { KeyEntry } from '../../../../src/types';
```

正确写法（✅）：

```ts
// ✅ 一律经 @datazen/ui / @datazen/driver-sdk
import { cn, useI18n } from '@datazen/ui';
import type { KeyEntry } from '@datazen/driver-sdk';
```

补充约束：

- **唯一实现原则**：某能力一旦下沉到 `@datazen/ui` / `@datazen/driver-sdk`，宿主原路径（如 `src/lib/cn.ts`、`src/lib/nativeContextMenu.ts`、`src/commands/driver.ts`）只允许保留**薄再导出**（re-export 指向 SDK 单实现），不允许出现第二份实现；驱动永远 import 包名，不 import 宿主薄再导出路径。**薄再导出只为存量宿主消费方而留**：下沉时若全仓已无宿主 import 该路径，则宿主文件**直接删除、不留空壳**（`driverSettings` 即此例——`packages/driver-sdk/src/driverSettings.ts` 是唯一实现，宿主旧路径已不存在），消费点一并改为直接 import SDK。
- **SDK 的宿主防腐层**：`packages/driver-sdk/src/index.ts` 内部仍会以相对路径包装少量宿主模块（如方言与 `src/types` 的部分 type-only 出口）。这是 SDK 作为防腐层的允许行为，但**驱动侧不得效仿**——驱动可见面只有两个包的公开导出。
- **过渡期例外（Wave 3 `i18n-drivers` 合并后已清零；本条为 Wave 4 护栏落地时的实测基线）**：`packages/drivers/**` 中指向宿主 `src/` 的**说明符字面量**共 **2 处 / 1 个文件**：
  1. **生产码：0 处**。历史上（基准 `8b66586e4` 之前）此处登记过 32 处宿主 `useI18n` 相对 import（redis 31 + `sqlserver/ui/ConnectionFields.tsx` 1）与 8 处 `vi.mock('<rel>/src/hooks/useI18n')`；`i18n-drivers` 轨已把它们全部换源为 `@datazen/ui` 的 `useI18n`（mock 改 partial-mock `@datazen/ui`），实测现网 `packages/drivers/**` 生产码零命中。
  2. **宿主集成测试夹具：2 处**——`packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5`（渲染宿主 `WebContextMenuHost`）与 `:9`（断言宿主 `contextMenuStore`，import 该模块同时触发 `bindContextMenuBridge`），协调者已裁决豁免、留待宿主菜单挂载经 SDK 桥下沉后移除。

  清点**不再使用** `grep -rn "from '\.\./.*src/" packages/drivers/*/ui/`：该形态只匹配 `from`，会漏掉 `vi.mock(...)` / 动态 `import(...)` / `require(...)` 的字符串参数（Wave 2 就是这样漏掉了 8 处 mock）。唯一口径是护栏脚本 `node scripts/check-driver-import-boundaries.mjs`（`pnpm test:boundaries`，见 2.6），它扫描全部说明符字面量，并把它上面这 2 处写死为脚本内 `ALLOWLIST` 的精确三元组（规则 + 文件 + 说明符，各带原因与归属里程碑）。**新增任何一条宿主 `src/` 引用即 exit 1 阻断 CI；豁免条目失效（文件被删或违规已被修）同样报错**，防止白名单腐烂。**除这 2 处外不存在任何豁免**（禁止目录级/通配级豁免）。

**阻断范围 = 本仓跟踪的源码（BUG-008 裁定）**：R1 只对**本仓 git 跟踪的源码**红。本地全量检出可能携带 gitignored 的外部仓库树——git 驱动 clone（`.gitignore` `/packages/drivers/*` 覆盖，如 `packages/drivers/superset/`）与已 stage 的 Pro EP（`packages/pro-extensions/`，每个子包是独立 git 仓库）——它们不是本仓代码。护栏在**发现违规时**对该文件逐个执行 `git check-ignore` 判定归属（绝不在遍历热路径上调用）：属外部树 → 降级为 **advisory**（仍逐条点名 `文件:行`、计入 advisory 总数，但**不影响 exit code**），输出追加 `external (untracked) repo — contract drift to be fixed in that repo, not here`，漂移由该仓库自行整改；git 不可用或报错一律按本仓跟踪处理（fail-closed，口子不会因环境损坏而放松）。**外部漂移不占 `ALLOWLIST` 额度**（上面 2 条夹具仍是唯一豁免）。当前外部漂移登记（本地 `--drivers=all` + Pro 检出实测）：**superset R1 ×2** —— `packages/drivers/superset/ui/SupersetConnectionFields.tsx:3` 与 `packages/drivers/superset/ui/SupersetSchemaTree.tsx:19`（均相对 import 宿主 `src/hooks/useI18n`，须在 superset 自身仓库换源 `@datazen/ui`，已移交、不属本轨整改范围）。

## 2.2 宿主能力取用模式（落点决策表）

驱动需要「宿主侧的东西」时，按下表决策，**按顺序**问自己：

| # | 问题的答案 | 落点 | 已有先例（可直接对照源码） |
| --- | --- | --- | --- |
| 1 | 纯函数 / 纯 IPC 封装，不读宿主 store、不依赖宿主模块状态？ | **下沉 `@datazen/driver-sdk`**（**移动**实现；宿主原路径**有存量消费方时**改薄再导出，**无消费方时直接移走、不留空壳**，见下方规则） | 留薄再导出壳：`src/lib/cn.ts` → `@datazen/ui`（整文件一行 `export { cn } from '@datazen/ui';`）；`src/lib/nativeContextMenu.ts:7-15` → `packages/driver-sdk/src/nativeContextMenu.ts`；`src/commands/driver.ts:6-11` → `packages/driver-sdk/src/ipc/driverCommands.ts`；`src/commands/file.ts:2/9` → 与 SDK `fileCommands` **合并再导出**（宿主另加 host-only 命令）。整体移走不留壳：`packages/driver-sdk/src/driverSettings.ts`（宿主 `src/lib/driverSettings.ts` 已于 `92a039383` 移走且不存在，宿主消费点 `JsonSchemaSettingsForm` / `DriverSettingsSection` 改为直接 import SDK） |
| 2 | 需要宿主的 zustand store / React hook 的**运行时状态或行为**？ | **建注入桥**：SDK 内新增 `xxxBridge.ts`，导出 `bindX()` + `useBoundX()`；宿主在 store/hook 定义处模块加载时 bind（见 2.3） | `settingsStoreBridge` / `connectionStoreBridge` / `confirmDialogBridge` / `schemaStoreBridge` |
| 3 | 纯共享**数据类型**（type-only）？ | **下沉 `packages/driver-sdk/src/types/*.ts`** 并从 index 导出；宿主源位置 re-export 兼容存量 | `types/kv.ts`（`KeyEntry` / `KeyScanResult`）、`types/menu.ts`、`types/connection-form.ts`、`types/connection-view.ts` |
| 4 | 只对宿主壳层有意义（窗口路由、面板布局、Tab 管理…）？ | **留在宿主**。驱动通过 props 回调（如 `ConnectionViewProps` 的 `ConnectionViewActions`）或 Driver Command 与之交互，宿主代码 import 驱动入口由 `generated.ts` codegen 完成 | `src/lib/connectionViews/types.ts`（re-export 自 SDK 类型） |
| 5 | 是对宿主核心表面（SQL 编辑器 / 图表）的特权深度增强，对键入延迟有严苛要求？ | **不是驱动，走 EP**：`@datazen/extension-points` 扩展点插槽（独立维度，见 [extensibility.md](../architecture/frontend/extensibility.md) 与 AGENTS.md 四维扩展体系） | `sqlEditorProEP` |

**为什么不做「bridge 式回退查表」**（防止后人重新引入）：任何「驱动查不到就悄悄回落到宿主实现/宿主字典」的隐式双源，都会在同一能力上产生宿主与包两份真值、掩盖真实耦合并使驱动无法独立编译与测试；显式 `bindX()` 把缺失绑定变成模块加载期的一次确定性抛错（`'<X> has not been bound to driver-sdk yet.'`），问题在开发期即刻暴露，而不是在用户机器上静默错乱。

## 2.3 能力注入桥清单与用法

### 2.3.1 现有 bridge 清单

| 能力 | SDK 模块 | 宿主注入 API | 驱动消费 API | 宿主 bind 时机（代码出处） |
| --- | --- | --- | --- | --- |
| 设置 store | `packages/driver-sdk/src/settingsStoreBridge.ts` | `bindSettingsStore(store)` | `useBoundSettingsStore`（selector / `getState` / `setState`） | `src/stores/settingsStore.ts` 文件末尾 `bindSettingsStore(useSettingsStore)` |
| 连接配置 store | `packages/driver-sdk/src/connectionStoreBridge.ts` | `bindConnectionStore(store)` | `useBoundConnectionStore`（selector / `getState`） | `src/stores/connectionStore.ts` 文件末尾 `bindConnectionStore(useConnectionStore)` |
| 确认对话框 | `packages/driver-sdk/src/confirmDialogBridge.ts` | `bindConfirmDialog(hook)` | `useBoundConfirmDialog(): [ConfirmDialogFn, ReactNode]` | `src/hooks/useConfirmDialog.tsx` 定义处 `bindConfirmDialog(useConfirmDialog)` |
| Schema store | `packages/driver-sdk/src/schemaStoreBridge.ts` | `bindSchemaStore(store)` | `useBoundSchemaStore` + `syncSchemaTables` / `syncSchemaNamespace` / `registerPathAliases` / `getCachedPathItems` / `cachePathItems` / `subscribeSchemaPathItems` | `src/stores/schemaStore.ts` 文件末尾 `bindSchemaStore(useSchemaStore)` |
| Web 右键菜单挂载 | `packages/driver-sdk/src/nativeContextMenu.ts` | `bindContextMenuBridge({ show, hide })` | `showNativeContextMenu` / `hideNativeContextMenu`（纯函数直接调用，无需 useBound*） | `src/stores/contextMenuStore.ts` 模块加载时 `bindContextMenuBridge({...})` |

约束（全部由现有实现强制，勿绕开）：

1. **bind 只发生在宿主侧、模块加载期**（store/hook 定义文件末尾），保证任何驱动 UI 被渲染时桥必已绑定；**驱动/测试从不调用 `bindX`**（单测例外：测试内可用 harness store 显式 bind，见 `packages/drivers/redis/ui/__tests__/useRedisGate.test.tsx` 的做法）。
2. 未绑定即消费 ⇒ 抛错 `'<X> has not been bound to driver-sdk yet.'`（`show/hideNativeContextMenu` 例外：`hide` 在未绑定时是安全的 no-op，因为 show 必先抛错、菜单不可能已打开）。
3. 消费形态模仿 zustand：`useBoundSettingsStore((s) => ...)` 在 React 组件内订阅；`useBoundSettingsStore.getState()` 在事件回调/异步路径命令式读取。桥类型只暴露驱动所需的**状态子集**（如 `SettingsBridgeState` 只有 `settings.safeMode / editorFontFamily / driverSettings`），新增字段须同时收窄评审。
4. SDK 包 `sideEffects: false`（`packages/driver-sdk/package.json`），因此**新增 bridge 模块不得引入顶层副作用**；宿主 bind 调用是宿主 store/hook 模块自身的顶层副作用，随该模块被宿主应用 import 而必然执行。

### 2.3.2 最小用法示例（与真实 API 签名一致）

宿主侧——在 store 定义文件末尾注入（真实出处：`src/stores/settingsStore.ts`）：

```ts
// 宿主 src/stores/settingsStore.ts（节选）
import { bindSettingsStore } from '@datazen/driver-sdk';

export const useSettingsStore = create<...>()(/* ... */);

// 模块加载即注入：驱动经 useBoundSettingsStore 读取，不 import 宿主代码
bindSettingsStore(useSettingsStore);
```

驱动侧——组件内订阅 + 事件路径命令式读取 + 确认对话框（真实出处：`packages/drivers/redis/ui/shared/SafeModeBadge.tsx`、`packages/drivers/redis/ui/shared/useRedisGate.ts`）：

```tsx
// 驱动 ui/xxx.tsx
import { useBoundSettingsStore, useBoundConfirmDialog } from '@datazen/driver-sdk';

function SafeModeBadge() {
  // React 订阅形态（zustand selector 形状）
  const safeMode = useBoundSettingsStore((s) => s.settings.safeMode);
  // ...
}

async function gateWrite(): Promise<boolean> {
  // 非渲染路径命令式读取
  const safeMode = useBoundSettingsStore.getState().settings.safeMode;
  const [confirm, dialog] = useBoundConfirmDialog(); // [ConfirmDialogFn, ReactNode]
  return confirm({ title: '...', message: '...', kind: 'warning' });
  // dialog 需在组件树中渲染一次
}
```

## 2.4 i18n 契约（单一运行时 + 词条自注册）

> 本节描述**终态契约**，且**已全部落地**：单一运行时与宿主 `setLocale` 接线由 `i18n-core` 轨交付，「驱动词条自注册」与「驱动 UI 换源 `@datazen/ui` 的 `useI18n`」由 `i18n-drivers` 轨交付（Wave 3 已合入 `feat/driver-decoupling`，实测：`packages/drivers/{redis,mongodb}/locales/index.ts` 存在、入口副作用行分别是 `packages/drivers/redis/ui/shared/meta.ts:4` 的 `import '../../locales';` 与 `packages/drivers/mongodb/ui/meta.ts:4` 的 `import '../locales';`、宿主 `DRIVER_LOCALES` 聚合链路已删）。设计红线：**`@datazen/ui` 是唯一 i18n 实现；没有 bridge 概念、没有兼容 re-export；只有宿主调用 `setLocale`；词条由各 package 自己提供并自注册；驱动侧 `t()` key 是普通 `string`。**

### 2.4.1 唯一运行时（已由 i18n-core 轨落地）

全部查表 / 回落 / 插值逻辑只存在于一处：`packages/ui/src/i18n.ts`（经 `@datazen/ui` 导出），公开 API 仅六个：

```ts
setLocale(locale: string): void;                       // 切换语言并通知订阅者
getLocale(): string;                                    // 当前激活 locale
registerTranslations(
  resources: Record<string, Record<string, string>>,
): void;                                                // 唯一词条注册入口（重复注册为后写覆盖合并）
t(key: string, params?: I18nParams): string;            // registry[locale] ?? registry['en'] ?? key，再做 {param} 插值
useI18n(): { t: typeof t; language: string };           // useSyncExternalStore 订阅 locale 变化的 React hook
getRegisteredTranslations(locale: string): Record<string, string>;  // 只读快照（浅拷贝）；未知 locale 返回 {}
```

- 第 6 个 API `getRegisteredTranslations` 由 Wave 3 `i18n-drivers` 轨有意新增（定义 `packages/ui/src/i18n.ts:69`，导出 `packages/ui/src/index.ts:27`）：返回该 locale 当前注册表的**浅拷贝**（宿主 eager + 已加载 lazy 域包 + 各驱动/扩展自注册词条），未知 locale 返回 `{}`，**不订阅** locale 变化 ⇒ 只服务工具 / 导出快照 / 测试，不是渲染路径。宿主消费方为 `src/locales/index.ts:11`（import）与 `:108`（`getAllTranslations` 唯一调用）；驱动侧与包内用例见 `packages/drivers/{redis,mongodb}/ui/__tests__/localePackRegistration.test.ts`、`packages/ui/src/__tests__/i18n.test.tsx`。
- 没有任何 Host↔包 locale bridge、没有第二套引擎、没有兼容 re-export 层（历史上 `@datazen/extension-points` 的 i18n bridge 模型已整体删除，该包不再导出任何 i18n 符号）。
- 宿主内部 `src/hooks/useI18n.ts` 是**宿主消费点别名**（re-export 自 `@datazen/ui`），仅宿主自身组件使用；驱动/扩展一律直接 `import { useI18n } from '@datazen/ui'`。

### 2.4.2 `setLocale` 只有宿主调用（已落地）

宿主**运行时接线**唯一入口：`src/lib/localeSync.ts` 的 `startLocaleSync()`——以 `settingsStore.settings.language` 播种（`:20`）并在其变化时调用 `setLocale`（`:24`）；由 `src/main.tsx` 启动时调用一次。

同一约束在宿主内部还有**一处**、且仅限非渲染路径的用法，读者易把它误读成「第二个接线点」：`src/locales/index.ts:69-84` 的 `getTranslation(locale, key, params)` 适配器，为查外语字典在同一次同步调用内临时 `setLocale(locale)`（`:78`）并在 `finally` 复位（`:82`）。它只服务工具/测试快照、不在 React 渲染路径，且**仍在宿主内**，因此不违反「只有宿主调用 `setLocale`」；驱动与扩展不得效仿这种临时换 locale 的写法（要换语言就走宿主设置）。

驱动、扩展（含 Pro EP）生产路径出现任何 `setLocale` 调用即违规。编译期无法强制此约束，由 Wave 4 import 护栏的 **R2** 规则 lint 兜底（见 2.6）：R2 扫描 `packages/**` 的全部 `.ts/.tsx/.js/.jsx/.mjs/.cjs`，豁免表是脚本内 `R2_FILE_CARVEOUTS` 的**精确文件清单**（**不是目录级豁免**）——`packages/ui/src/i18n.ts`（`setLocale` 唯一实现所在文件，`:34`）与它自己的单测 `packages/ui/src/__tests__/i18n.test.tsx`（必须能调用才能测）。也就是说 `@datazen/ui` 包内**其它**组件出现 `setLocale(...)` 调用同样红。宿主 `src/**` 天然不在 R2 范围内，因为宿主正是唯一合法调用方。注释文字、字符串与契约声明（`setLocale(locale: string): void;`）都不算调用。当前实测：**本仓跟踪的** `packages/**` 源码中 `setLocale(` 只命中上述两个文件（定义处 1 行 + 单测调用 **15** 行），其余为 **0**。该单测调用行数会随用例扩写而增长，**行数的权威落点即该文件本身**（`packages/ui/src/__tests__/i18n.test.tsx`），上列 15 为本次回扫（2026-09-21）快照值；复核口径：`grep -c "setLocale(" packages/ui/src/__tests__/i18n.test.tsx` 的命中数须扣除注释提及行（当前 16 − `:100` 的 JSDoc 注释 1 = **15** 个调用行；`:7` 的 `import` 引用行不计）——R2 豁免是文件级清单，该文件内行数变化不触发规则。

R2 的阻断范围与 R1 同为「本仓跟踪的源码」（BUG-008 裁定，机制见 2.1.2 末段与 2.6）：gitignored 外部树中的违规降级为 advisory、不阻断门禁，也不进 `ALLOWLIST`。当前外部漂移登记：已 stage 的 Pro EP `packages/pro-extensions/sql-editor-pro` 共 **6 处 R2 advisory**——`src/intentions/__tests__/intentionCodeActions.test.ts:119,141` 与 `src/locales/__tests__/locales.test.ts:28,32,36,39`（均为该 EP 自身单测切语言的合法用法；是否收敛由 editor-pro 仓库自行裁定，本仓不代其豁免、也不要求即刻整改）。

### 2.4.3 词条归属：各 package 自注册

| 词条集合 | 拥有者 | 注册方式 |
| --- | --- | --- |
| 宿主 UI 词条 | `src/locales/*`（领域包结构） | `src/locales/index.ts` 模块加载时把 eager 字典 `registerTranslations` 灌入共享注册表；lazy 域包经 `useLocaleDomains` / `ensureLocaleDomains` 按需注册 |
| 驱动词条 | `packages/drivers/<id>/locales/`（如 redis、mongodb 各语言文件） | **自注册（已落地）**：纯副作用模块 `packages/drivers/<id>/locales/index.ts` 静态 import 本目录全部语言字典后一次性 `registerTranslations({...})`；由该驱动 UI 的入口模块（即 `generated.ts` 实际 import 的首个驱动 UI 模块）挂一行指向本包 `locales/` 目录的副作用 import，**相对层级随入口目录深度而定**：入口在 `ui/meta.ts`（如 mongodb）写 `import '../locales';`（`packages/drivers/mongodb/ui/meta.ts:4`），入口在 `ui/shared/meta.ts`（如 redis，嵌套两层）写 `import '../../locales';`（`packages/drivers/redis/ui/shared/meta.ts:4`）。驱动一经装载即完成注册，宿主不需要知道驱动有哪些语言包 |
| Pro 扩展词条 | `packages/pro-extensions/*` 各自 locales | 直接 `import { registerTranslations } from '@datazen/ui'`（EP 经 `globalThis.__DATAZEN_HOST__['@datazen/ui']` 与宿主共享同一单例，见 `src/main.tsx`）；与宿主 key 同名时的覆盖语义与处置见 2.4.4 观察项（BUG-004） |

配套终态（**已随 Wave 3 合并落地**，Wave 4 回扫时按实测改写）：

- 宿主端 `DRIVER_LOCALES` 聚合链路**已整体删除**：`src/extensions/generated-locales.ts` 及其在 `scripts/resolve-drivers.mjs` 中的 codegen、相关脚本引用一并移除——实测 `scripts/` / `src/` / `packages/` / `e2e/` 中 `DRIVER_LOCALES`、`generated-locales` 的**生产码**引用 **0 命中**（口径与同批回扫的 `docs/development/independent-driver-development.zh-CN.md:214`「生产码已无该标识符」一致）；仍留 **4 处已知残留**——3 处散文/忽略规则（`AGENTS.md`、`CONTRIBUTING.md`、`.gitignore`，属他轨文档，已登记交协调者）与 1 处代码内常量（`scripts/check-driver-import-boundaries.mjs:94` 的 `SKIPPED_CODEGEN_FILES` 仍列着已不存在的 `src/extensions/generated-locales.ts`；该条目由 Wave 4 `import-guard` 轨晚于本行基线所写，属无害的防御性写法，本仓不改护栏以免牵动其 36 例夹具复验）——不存在「宿主替驱动收集词条」这一步。
- 语言 code 字面量与宿主保持一致（`zh-CN`、`zh-TW`、`pt-BR` 一律带连字符）。核对时注意**「仓库里有语言文件」≠「宿主已接线该语言」**，真值分三层各取不同出处：
  - **宿主实际接线的内置语言只有 `en` 与 `zh-CN`**：`src/locales/builtinLocales.ts:9` 的 `BUILTIN_LOCALES = ['en', 'zh-CN']`（真值源 `src/locales/builtin-locales.json`；`BUILTIN_LOCALE_LABELS` 同文件 :26-29 亦只有这两项；`src/locales/fullLocales.ts` 供测试/工具用，同样只含这两个）。
  - **其余 8 个语言目前只做 parity 校验、未进 `BUILTIN_LOCALES`**：`de`、`es`、`fr`、`ja`、`ko`、`pt-BR`、`ru`、`zh-TW`，以文件形态存在于 `src/locales/`（如 `src/locales/pt-BR.ts` + `src/locales/pt-BR/`），由 `scripts/i18n-sync-check.mjs:36` 的 `LOCALE_FILES` 逐个列表做词条校验；**就宿主 `src/locales/` 的这 8 个语言文件而言**，除 `src/locales/` 内部再导出外生产路径无运行时 import（此限定只描述宿主词条，不适用于驱动包，见下条不对称说明）。
  - 因此 **`zh-CN` 的连字符以 `BUILTIN_LOCALES` 为出处，`pt-BR` 的连字符以 `LOCALE_FILES`（`scripts/i18n-sync-check.mjs:36`）与语言文件名（`src/locales/pt-BR.ts`、`packages/drivers/redis/locales/pt-BR.ts`、`packages/drivers/mongodb/locales/pt-BR.ts`）为出处**。驱动包 `locales/` 现覆盖 10 个语言文件（redis、mongodb 各 10），其文件名必须与宿主同名同分隔符；新增语言只加文件，不改宿主 `BUILTIN_LOCALES`（除非该语言确已接线）。
- **上述三层全部只是宿主侧口径；驱动侧的注册集合有意与之不对称，不是越界**。宿主运行时接线的可选语言是 `en` / `zh-CN`（第 ① 层），而驱动 `locales/index.ts` 是静态 import 本目录**全部**语言字典后一次性灌入共享注册表（redis、mongodb 各 10 语言），驱动一经装载即在应用启动时全部进入 main chunk——不存在「驱动跟着宿主只注册 2 语言」这一形态；本契约同样不要求、不建议驱动改走惰性 / 按需注册——O-1 裁定明令禁止引入该机制，全量 eager 注册即终态。
  - **代价与裁定出处**：该包体代价已由并行轨 `i18n-drivers` 的 **O-1 裁定**接受并写为终态（该轨 `docs/development/coordination/tracks/i18n-drivers/bugs.md` 观察项 O-1，同一 config、仅切换 locale 装配的三档 `vite build` 实测：完全不装配驱动词条 1,501.93 kB min / 仅 `en` + `zh-CN` 1,528.55 kB / 全部 10 语言 1,605.12 kB，净增 **+76.57 kB min、+6.59 kB gzip**，均在 main chunk）。
  - **「是否注册」与「运行时是否可达」是两件事**：词条「是否被注册进共享注册表」由驱动自己决定（全量注册、eager）；某语言「在运行时是否可达」则由宿主的**可选语言集合**决定，与驱动注册了多少语言无关——该集合是 `BUILTIN_LOCALES`（第 ① 层）并上宿主扩展经 `registerLocale()` 注册的语言（`src/locales/index.ts:47-50` 写入 `extensionLocales`，经 `getExtensionLocales()` 汇入设置页语言下拉，见 `src/windows/settings/SettingsContent.tsx:91-100`）。注意宿主的惰性域包机制（`src/locales/lazyPacks.ts:21-34`，域清单 `src/locales/domains.ts:21` 的 `sync` / `workflows` / `dashboard` / `mcp`）只服务**宿主词条**、且只覆盖 `en` / `zh-CN`，**驱动词条不经它装载**。因此驱动多出的 8 个语言在当前宿主集合下运行时不可达，其字典常驻注册表，属为「未来经 `registerLocale()` 接入第 3 语言」预付的容量（O-1 裁定接受代价时的收益侧）。
  - **评审口径（两句结论）**：两者不等价——评审驱动包时不得据宿主接线集合判定驱动注册了多余语言，也不得据驱动注册集合推断宿主已支持该语言。

### 2.4.4 key 命名与类型

- 驱动词条 key 必须带**驱动自有前缀**（现状：`redis.*`、`mongo.*`），与宿主前缀互斥，保证合并进同一注册表不碰撞；新增 key 只改本包 `en.ts`（唯一 source of truth），其余语言由同步工具补齐。
- **观察项（BUG-004，Wave 4-B 全量回归实测；本条不构成对 EP/wapp 的强制条款，仅留档 + 交叉引用）**：`packages/pro-extensions/sql-editor-pro` 自带 **26 个 key**（24 个 `query.*` + 2 个 `settings.editor.intention*`），其中 **5 个与宿主同名且同为 `query.*`**：`query.params`、`query.paramValue`、`query.editor.param.historyLabel`、`query.editor.param.clearHistory`、`query.editor.drop.crossConnection`（en 侧 3 异值 + 2 同值，zh-CN 侧 5 个全部异值；宿主侧这 5 个 key 当前 **0 消费方**，实为宿主字典里的孤儿条目）。EP 的 `src/locales/index.ts:11` 在宿主 eager 注册**之后**才 `registerTranslations({ en, 'zh-CN': zhCN })`，按 2.4.1 的「后写覆盖 + 逐 key 合并」语义，装了 Pro 的构建对这 5 个 key 取 EP 文案、Community 构建取宿主文案 ⇒ 一旦宿主新增任一同名 `query.editor.param.*` 消费点，同一 UI 会在两个版本间出现文案分歧，且没有任何静态检查会红。**处置**：命名空间归属（是否收敛为 `pro.*` 前缀）移交 `sql-editor-pro` 自身仓库裁定（本仓不代其豁免、也不即刻要求整改），本契约为 EP/wapp 明确**不加**强制前缀要求；触发条件出现前无用户可见后果。该观察项同时说明 2.4.3「Pro 扩展词条」行的注册方式目前只约束「谁注册」，不约束「key 归属」。（wapp 侧同批实测：`packages/wapps/**` 内 `registerTranslations` / `@datazen/ui` 均 **0 命中**，即 R-5 的后半句「wapp 尚无自带词条」成立。）
- 驱动侧 `t()` 的 key 是**普通 `string`**：没有编译期 `I18nKey` 字面量联合约束（宿主 `I18nKey` 是宿主内部编译期约束，与驱动无关），驱动组件中不得出现 `as I18nKey` 之类宿主类型断言。
- 词条完整性（parity）由脚本扫描保证（**已落地**）：`node scripts/i18n-sync-check.mjs`（`scripts/__tests__/i18n-sync-check.test.mjs` 覆盖其分支）以各包 `en.ts` 为 source 校验其余语言文件 key 集合一致，扫描**两类**集合——宿主 `src/locales/`（`scripts/i18n-sync-check.mjs:33`）与驱动包 `packages/drivers/<id>/locales/`（同文件 `:34` 的 `driversDir`，经导出的 `checkDriverLocalePacks()` 在 `:318` 汇入报告）；驱动包缺 `locales/index.ts`、或 `index.ts` 漏 import 某个语言文件，都计为结构性问题并让脚本非零退出。开发期间只改本包 `en.ts`，其余语言由 i18n-sync 流程补齐。

### 2.4.5 驱动 UI 消费写法

```tsx
import { useI18n } from '@datazen/ui';

function RedisConsole() {
  const { t } = useI18n();
  return <span>{t('redis.console')}</span>;        // 普通 string key
}
```

非 React 上下文（如校验器）不 import i18n——由宿主/SDK 把 `t` 作为参数注入，先例：`@datazen/driver-sdk` 的 `DriverFormValidator` 类型第二参数即为 `t: (key: string) => string`。

### 2.4.6 语言切换行为

语言切换 = 宿主 settingsStore `language` 变化 → `setLocale` → 所有 `useI18n` 消费组件（宿主、驱动、EP）即时重渲染；驱动 UI 无需任何监听代码，也不允许自行持久化语言偏好。

## 2.5 新增宿主依赖时的标准流程

1. 按 2.2 决策表选落点；
2. 若下沉纯函数/IPC：**移动**实现进 SDK（禁止复制），宿主原路径**有存量消费方则改薄再导出、无消费方则连文件一并删除**，全仓保持单实现；
3. 若建注入桥：SDK 新增 `xxxBridge.ts`（`bindX` + 未绑定抛错 + `useBoundX` 收窄类型），宿主在对应 store/hook 定义处 bind，并为桥补 SDK 侧单测（先例：`packages/driver-sdk/__tests__/`）；
4. 驱动侧只 import 包名并更新本文件 2.3.1 清单表；
5. 生产代码零 `../../../src/` 新增（已由 Wave 4 护栏 **R1** 强制，见 2.6；确属宿主集成测试夹具而必须保留时，须在 2.1.2 与护栏 `ALLOWLIST` 同步登记精确三元组）。

## 2.6 Wave 4 import 护栏（已落地）

上述禁止项已由构建期护栏脚本强制执行：

| 落点 | 值 |
| --- | --- |
| 脚本 | `scripts/check-driver-import-boundaries.mjs`（导出纯函数 + `runCli()` + `process.argv[1].endsWith(...)` main 守卫，与 `check-id-terminology.mjs` / `check-module-layers.mjs` 同构） |
| npm script | `pnpm test:boundaries` |
| CI | `.github/workflows/ci.yml` 步骤 **`Guard driver/host import boundaries`**（位于 `Guard version consistency` 之后、`Guard i18n sync (warning only)` 之前） |
| 本地等价 | `scripts/ci-local.sh` 步骤 `3.3/11 Guard: driver/host import boundaries`；`scripts/run-full-automation-test.sh` Stage 1 的 `pnpm test:boundaries`；`scripts/run-regression.sh` 步骤 `1/7`（合并前全量门禁首步，秒级失败即停） |
| 单测 | `scripts/__tests__/check-driver-import-boundaries.test.mjs`（**36 例**：内联虚拟文件树 fixture 覆盖三条规则、豁免路径与 BUG-008 跟踪域分类〔ignored 外部树降 advisory / tracked 照样 blocking / 虚拟树默认全 tracked / 真实仓 predicate 命中并缓存 / git 不可用 fail-closed〕 + 真实仓库 `runCli` 用例。本轨新增脚本实测覆盖：行 **100%** / 语句 99.31% / 分支 95.72% / 函数 100%。未覆盖语句仅 2 处，位于 `walk()` 的真实文件系统目录遍历过滤器（`:444` 的 `SKIP_DIR_NAMES` continue、`:450` 的 `!entry.isFile()` continue）；其余 6 处未触发分支是 `??`/默认参数兜底（`:275`/`:305` 转义符恰在文件末尾、`:379` 取行文本兜底、`:524`/`:525` `opts.log`/`opts.error` 默认值、`:630` `opts.argv` 默认值），均非规则与豁免逻辑） |

三条规则与退出码：

- **阻断范围（BUG-008）**：`blocking: true` 的规则（R1/R2）**只对本仓 git 跟踪的源码红**。分类在发现违规时对单个文件执行 `git check-ignore`（`createGitIgnorePredicate`，带缓存；绝不在遍历 1400+ 文件的热路径上调用），命中 gitignored 外部树（git 驱动 clone、`packages/pro-extensions/*`）即降级为 advisory，逐条点名并追加 `external (untracked) repo — contract drift to be fixed in that repo, not here`，不影响 exit code；漂移由该仓库自行整改，**不新增 `ALLOWLIST` 条目**。git 不可用/报错一律按 tracked 处理（fail-closed，门禁不会因环境损坏被放松）。虚拟文件树（单测）默认全部视为 tracked，除非测试注入 `isIgnored`。本地 `--drivers=all` + Pro 全量检出的当前外部漂移：superset R1 ×2 + editor-pro R2 ×6（文件:行见 2.1.2 / 2.4.2），连同 R3 基线 4 处共 **12 条 advisory、exit 0**。
- **R1**（阻断）驱动包禁引宿主：扫描 `packages/drivers/**`（`ui/**`、`locales/**`、`e2e/**` 全含）内**所有说明符字面量**并做相对路径解析——`import` / `export … from` / 动态 `import()` / `vi.mock` / `vi.doMock` / `require` 以及任何以模块路径为参数的辅助函数一律同等对待，解析结果落进宿主 `src/` 即违规。这是 2.1.2 那条 `grep "from '…'"` 口径被抛弃的原因：只匹配 `from` 会漏掉 mock/require 形态。
- **R2**（阻断）非宿主禁调 `setLocale(`：扫描 `packages/**`，豁免只有脚本内 `R2_FILE_CARVEOUTS` 列出的**两个精确文件**——`setLocale` 的定义文件 `packages/ui/src/i18n.ts` 与它自己的单测 `packages/ui/src/__tests__/i18n.test.tsx`（不是整包 `packages/ui/**` 目录级豁免，`@datazen/ui` 其它组件调用照样红，见 2.4.2）；注释、字符串、契约成员声明（`setLocale(locale: string): void;`）与 `import { setLocale }` 这类不带括号的引用均不算调用。
- **R3**（**advisory，暂不阻断**）宿主禁引驱动内部：`src/**` 相对解析进 `packages/drivers/**` 即列出，跳过 gitignored codegen `src/extensions/generated{,-locales,-pro}.ts`（那是唯一被允许的宿主→驱动边，见 2.2 第 4 行）。**现状基线非 0**（4 处：`src/locales/locales.test.ts:107`、`src/test/driverUiSetup.ts:25,26`、`src/windows/connection/DocumentConnectionView.tsx:25`），是否收紧（改造或另立豁免）已交协调者裁定；裁定前 R3 只报告不失败，`RULES.R3.blocking` 翻为 `true` 即收紧。
- 退出码：`0` 干净（含「仅外部树 advisory / R3 advisory」的情形） · `1` 存在阻断违规（限本仓跟踪源码，见上条 BUG-008 阻断范围）或**过期豁免** · `2` 一个文件都没扫到（防「扫描器失效却报成功」）。豁免写在脚本内 `ALLOWLIST` 常量里，形如 `(规则, 文件, 说明符)` 精确三元组 + 原因 + 归属里程碑，**禁止目录级/通配级豁免**；条目所指文件消失或违规已修，同样按过期豁免报错。

## 2.7 契约自查清单（Reviewer / CI 预备）

- [ ] `pnpm test:boundaries` 绿：驱动可见面无任何 `.../src/` 形态宿主 import。现网基线（Wave 4 护栏实测口径，非 `grep "from '…'"` 那种会漏 mock 的旧口径）= **生产码 0 处 + 夹具 2 处**（`packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5,9`，见 2.1.2）；**护栏 ALLOWLIST 只允许这 2 条，出现第 3 条即新增违规**。阻断只看本仓跟踪的源码：gitignored 外部树（git 驱动 clone、`packages/pro-extensions/*`）的命中以 advisory 报告、由该仓库自行整改（BUG-008，见 2.1.2 末段与 2.6），评审时既不得借此在外部树上放行新违规，也不得因外部漂移红本仓门禁。
- [ ] 驱动 UI 的组件/工具/类型仅来自 `@datazen/ui`、`@datazen/driver-sdk`、`@datazen/extension-points`（仅 EP 类型）、npm 依赖。
- [ ] 新共享类型为移动而非复制，宿主存量 import 零改动（薄 re-export）。
- [ ] 新 bridge 具备：宿主模块加载期 bind、未绑定抛错文案、消费侧类型收窄、SDK 侧单测。
- [ ] 驱动词条带自有前缀，只改本包 `en.ts`，经本包 `locales/index.ts` 自注册（不新增宿主聚合 codegen）。
- [ ] 驱动/扩展代码零 `setLocale` 调用（同一 `pnpm test:boundaries` 的 **R2** 阻断，豁免仅 2.4.2 所列两个精确文件）；i18n 一律 `import { t | useI18n } from '@datazen/ui'`。
- [ ] 宿主侧不新增对 `packages/drivers/**` 的相对 import（R3 目前为 advisory 报告项，见 2.6；已列出的 4 处基线等待协调者裁定，评审时不得默认放行新命中）。
