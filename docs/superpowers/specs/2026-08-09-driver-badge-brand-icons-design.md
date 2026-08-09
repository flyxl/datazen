# 设计：驱动角标 — 品牌标优先，无品牌时父图 + 简写叠加

**日期：** 2026-08-09  
**状态：** 已实现  
**实现计划：** [docs/superpowers/plans/2026-08-09-driver-badge-brand-icons.md](../plans/2026-08-09-driver-badge-brand-icons.md)  
**关联：** [新建连接驱动图标](2026-08-09-new-connection-driver-icons-ui-design.md)、主题包 `db.*` 解析链

## 目标

1. 每个 `databaseType` **优先使用自身品牌 SVG**，禁止在「明明可以有独立识别」时静默整图复用父驱动图标。
2. 找不到可靠品牌标时，使用 **父驱动图标 + 右下角 `shortLabel` 简写**（运行时叠加），与连接列表角标语义一致：一眼看出家族与具体类型。
3. 为主驱动包内当前缺失独立文件的协议复用类型补齐品牌标（能找到官方/可合法使用的素材时）。

## 非目标

- 主题包商店 / CDN。
- 修改主题包覆盖优先级（主题包仍可覆盖任意 `db.<type>`）。
- 为 Git 驱动强制打包品牌标（有 `icons/{dbType}.svg` 则用；无则走本设计的父图叠加或色块占位）。
- 重做 `DbTypeBadge` 以外的列表布局。

## 已确认决策

| 议题 | 选择 |
|------|------|
| 总体方案 | **B：运行时叠加**（非预烘焙复合 SVG） |
| 品牌标来源 | 官方品牌资源 / 可合法嵌入的单色 mark，套入现有 24×24、`rx=5` 圆角色底风格 |
| 无品牌回退 | 父驱动底图 + 右下角 `shortLabel` |
| 再下一层回退 | 现有 Host 占位：`shortLabel` + `iconBg` 色块（无父图可解析时） |
| 生成侧 | 去掉「无文件则 alias 成父文件 URL」的静默行为；改为显式父映射供 UI 叠加 |

## 现状问题

`scripts/resolve-drivers.mjs` 中 `DRIVER_ICON_ALIASES` 在缺少 `icons/{dbType}.svg` 时，把下列类型直接映射到父文件：

| dbType | 当前 alias |
|--------|------------|
| `questdb`, `cloudberry` | `postgresql.svg` |
| `doris`, `starrocks`, `manticore`, `ob_oracle` | `mysql.svg` |

结果：`DbTypeBadge` 只渲染一张与父类型完全相同的图，无法区分协议复用类型。`mariadb` 已有独立 SVG，不受影响。

## 解析链（更新）

```
主题包 icons["db." + databaseType]
  → 驱动自有 icons/{dbType}.svg（DRIVER_ICON_ENTRIES）
  → 父驱动图标 + 右下角 shortLabel（仅当存在显式父映射且父图可解析）
  → Host 占位 shortLabel + iconBg
```

说明：

- **有自有品牌文件时绝不叠加简写**（整图即为该类型品牌标）。
- **父图叠加仅用于无自有文件的协议复用类型**；独立主类型（如 `redis`）无文件时仍走色块占位，不发明虚假父关系。

## 架构

### 1. 生成：`resolve-drivers.mjs`

**删除 / 停用** 将 alias 结果写入 `DRIVER_ICON_ENTRIES` 的路径（即：候选列表不再用 alias 文件顶替 `db.<type>` 条目）。

**保留** 静态表（可改名，如 `DRIVER_ICON_PARENT`）表达协议复用父子关系，例如：

```js
const DRIVER_ICON_PARENT = {
  questdb: 'postgresql',
  cloudberry: 'postgresql',
  doris: 'mysql',
  starrocks: 'mysql',
  manticore: 'mysql',
  ob_oracle: 'mysql',
};
```

生成物新增（空 stub 时为空对象）：

```ts
/** Protocol-reuse types without own badge SVG: parent dbType for composite badge. */
export const DRIVER_ICON_PARENTS: Record<string, string> = {
  // e.g. questdb: 'postgresql' — only when icons/questdb.svg is missing
};
```

规则：

- 若 `icons/{dbType}.svg` **存在**：写入 `DRIVER_ICON_ENTRIES['db.'+id]`；**不**写入 `DRIVER_ICON_PARENTS`。
- 若 **不存在** 且该 id 在父映射表中：写入 `DRIVER_ICON_PARENTS[id] = parentId`；同时确保父图仍可通过 `DRIVER_ICON_ENTRIES['db.'+parentId]` 解析（父自己必须有 SVG，或父再走占位）。
- 若既无文件也无父映射：两表皆无条目 → UI 色块占位。

Git stub / `drivers:restore` 后：`DRIVER_ICON_ENTRIES` 与 `DRIVER_ICON_PARENTS` 均为空对象（与现有 stub 约定一致）。

### 2. 前端暴露

`getDriverIconMap()` 不变。新增：

```ts
export function getDriverIconParents(): Record<string, string> {
  return { ...DRIVER_ICON_PARENTS };
}
```

（或等价导出；bootstrap / theme pack 应用无需把 parents 塞进主题包。）

### 3. `DbTypeBadge` 渲染

伪逻辑：

```
resolved = iconResolver.resolve(`db.${databaseType}`)
if resolved.kind === 'url' → <img> 整图品牌标
else:
  parentId = DRIVER_ICON_PARENTS[databaseType]
  parentResolved = parentId ? resolve(`db.${parentId}`) : null
  if parentResolved?.kind === 'url':
    → 相对容器 + 父 <img> + 右下角简写气泡（meta.shortLabel，背景可用 meta.iconBg）
  else:
    → 现有 placeholder（shortLabel + iconBg）
```

视觉约束（与现有 24/36 尺寸兼容）：

- 容器 `relative`，尺寸与现有 `size` 一致，圆角与阴影保持。
- 父图标铺满容器（`object-contain`）。
- 右下角简写：小胶囊/方块，字号约 `size` 的 35%–40%，最大约 11px；`shortLabel` 通常 2 字符；对比色文字（白）+ `iconBg`；略超出底边可接受但不得遮挡大半父标。
- `aria-hidden` 保持；辨识依赖可见简写 + 父品牌轮廓。

### 4. 品牌 SVG 资产（本次尽量补齐）

在下列路径新增 **独立** `{dbType}.svg`（风格对齐现有：`viewBox="0 0 24 24"`、圆角色底 + 居中浅色 mark）：

| 文件 | 实现结果 |
|------|----------|
| `packages/drivers/postgres/ui/icons/questdb.svg` | 已加入（QuestDB 公开 logo 嵌套） |
| `packages/drivers/postgres/ui/icons/cloudberry.svg` | 已加入（Cloudberry artwork white logomark） |
| `packages/drivers/mysql/ui/icons/ob_oracle.svg` | 已加入（OceanBase filled mark） |
| `packages/drivers/mysql/ui/icons/doris.svg` | **未加入**（无可合法嵌入的官方 mark；走父图+`Do`） |
| `packages/drivers/mysql/ui/icons/starrocks.svg` | **未加入**（同上；走父图+`Sr`） |
| `packages/drivers/mysql/ui/icons/manticore.svg` | **未加入**（同上；走父图+`Mc`） |

素材优先级（实现时遵循）：

1. 项目官方 brand / artwork 仓库或官网提供的 SVG mark（必要时裁切/单色化以适配 24×24）。
2. 若某类型 **无法** 取得可合法嵌入的品牌 mark：不硬画仿冒商标；留给 `DRIVER_ICON_PARENTS` + 右下角简写。

补齐成功的类型在 inject 后 **不应** 再出现在 `DRIVER_ICON_PARENTS` 中。

### 5. 文档与测试

- 更新 `src/assets/db-icons/README.md` 与（如需要）旧 new-connection 设计中「协议复用可共用父图」的表述，指向本规格。
- 单测：
  - `getDriverIconParents` / 生成约定：有自有 SVG 时 parents 无该 key；无 SVG 有父映射时有 key。
  - `DbTypeBadge`：自有 URL → 仅 img；无 URL 有 parent → 父 img + 简写文本；皆无 → placeholder。
- 手工：新建连接侧栏与主窗口连接列表中，6 个复用类型互不相同；有品牌文件时无角标叠加。

## 组件边界

| 单元 | 职责 |
|------|------|
| `resolve-drivers.mjs` | 扫盘生成 `DRIVER_ICON_ENTRIES` + `DRIVER_ICON_PARENTS` |
| `databaseTypes.ts` | 暴露 map / parents 给 UI |
| `iconResolver` | 仍只解析「最终 URL / placeholder」；**不**负责叠加 |
| `DbTypeBadge` | 唯一负责「父图 + 简写」合成 UI |

## 风险与约束

- **商标**：仅嵌入可合理用于产品内识别的官方/许可素材；不确定则走父图+简写，不仿冒。
- **主题包**：若主题覆盖了 `db.doris` 则整图替换、无叠加；若只覆盖 `db.mysql` 而 doris 无自有图，doris 叠加会用到被覆盖后的父图 URL（可接受：家族底图跟主题）。
- **stub CI**：`check-managed-stubs` 须允许新导出 `DRIVER_ICON_PARENTS` 空对象，与 `DRIVER_ICON_ENTRIES` 同等对待。

## 验收标准

1. 有独立品牌 SVG 的类型：角标为该品牌图，无右下角简写叠加。
2. 无独立 SVG 但有父映射的类型：显示父品牌图 + 右下角该类型 `shortLabel`，不再与父类型完全相同。
3. 无 SVG、无父映射：仍为 shortLabel 色块。
4. git 中 `generated.ts` / `plugin_init.rs` 保持空 stub；本地 inject 后可见正确条目。
