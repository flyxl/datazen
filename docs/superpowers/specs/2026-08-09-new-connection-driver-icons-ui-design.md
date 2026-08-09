# 设计：新建连接窗口 — 驱动图标、侧栏滚动与搜索

**日期：** 2026-08-09  
**状态：** 已实现  
**实现分支：** `feat/new-connection-driver-icons-ui`  
**实现计划：** [docs/superpowers/plans/2026-08-09-new-connection-driver-icons-ui.md](../plans/2026-08-09-new-connection-driver-icons-ui.md)

## 目标

1. 各数据库类型使用**驱动自带品牌图标**（不再用统一 Lucide `Database`）。
2. 修复新建连接窗口 UI：
   - macOS Overlay 下 traffic lights 区域「透出」列表（整页滚动导致内容上滚）。
   - 左侧驱动列表**独立滚动**，不带动整个窗口。
3. 左侧列表顶部增加**搜索框**（无单独 label，仅 placeholder）。

## 非目标

- 主题包商店 / CDN。
- 修改 macOS `TitleBarStyle::Overlay` 全局策略。
- 重做右侧连接表单字段。
- 为未纳入本次构建的 Git 插件驱动强制打包图标（路径与 path 驱动一致即可）。

## 已确认决策

| 议题 | 选择 |
|------|------|
| 图标归属 | **方案 A**：`packages/drivers/*/ui/icons/{dbType}.svg`，经 `resolve-drivers` 生成 |
| Host 中央 `src/assets/db-icons/` | 逐步由生成条目替代；`getDriverIconMap()` 以 generated 为准 |
| 协议复用类型 | **已由** [driver-badge-brand-icons](2026-08-09-driver-badge-brand-icons-design.md) **取代**：优先自有品牌 SVG；无则父图 + 右下角 shortLabel（不再静默整图 alias） |
| 列表渲染 | `DbTypeBadge`（解析链：主题包 → 驱动默认 → shortLabel 占位） |
| 搜索 UI | **无 label**；`placeholder` 承载提示（i18n key） |
| 搜索匹配 | `label` 与 `databaseType` id，不区分大小写；子串匹配 |

## 图标架构

### 解析顺序（不变）

```
主题包 icons["db." + databaseType]
  → 驱动默认（DRIVER_ICON_ENTRIES / getDriverIconMap）
  → Host 占位 shortLabel + iconBg
```

### 包内布局

```
packages/drivers/{driver}/ui/
  meta.ts
  icons/
    {dbType}.svg    # 例：mongodb.svg、clickhouse.svg
```

复用线协议的类型可：

- 在 meta 包中放独立 SVG，或  
- 在生成阶段把 `db.doris` 映射到与 `db.mysql` 相同的 URL。

### 生成物

`resolve-drivers.mjs` 写入 `src/plugins/generated.ts`：

```ts
export const DRIVER_ICON_ENTRIES: Record<string, string> = {
  'db.postgresql': new URL('...', import.meta.url).href, // 或 Vite ?url 静态 import
  // …
};
```

实现时优先与现有 Vite/`generated.ts` 模式一致（静态 `import … from '…?url'` 列表由脚本生成）。

`getDriverIconMap()`：

```ts
export function getDriverIconMap(): IconSourceMap {
  return { ...DRIVER_ICON_ENTRIES };
}
```

## 新建连接窗口布局

```
┌─ TitleBar（固定，opaque bg-titlebar）─────────────┐
│ [traffic lights pad]     新建连接                  │
├─────────────┬─────────────────────────────────────┤
│ aside       │ main                                │
│ 标题文案    │ 表单区 overflow-y-auto               │
│ [搜索框]    │                                     │
│ ┌列表──────┐│                                     │
│ │ scroll   ││                                     │
│ │ DbTypeBadge + label                             │
│ └──────────┘│                                     │
│             │ footer 固定                         │
└─────────────┴─────────────────────────────────────┘
根：h-screen overflow-hidden；aside/main：min-h-0
```

### 滚动修复要点

- 根容器：`h-screen min-h-0 overflow-hidden flex flex-col`。
- 内容行：`flex min-h-0 flex-1`。
- 侧栏：`flex min-h-0 flex-col`；**仅列表容器** `flex-1 min-h-0 overflow-y-auto`。
- 搜索框与「选择数据库类型」文案在侧栏顶部，**不随列表滚动**（或随侧栏顶栏固定，列表单独滚——推荐后者）。

Traffic lights「透出」随整页滚动消失；TitleBar 保持 `bg-titlebar` 不透明。

### 搜索

- 位置：侧栏顶部文案**下方**、列表**上方**。
- 控件：单行 `input`，**无可见 label**（可用 `aria-label` 等同文案以满足无障碍）。
- `placeholder`：i18n，例如「搜索驱动…」/ `Search drivers…`。
- 过滤：`label` 与 `value`（databaseType）`toLowerCase().includes(query)`。
- 无匹配：列表区短空态文案。

## i18n

新增 keys（en + zh-CN 完整；其余语系按仓库惯例补齐或脚本同步）：

- `newConn.searchDrivers` — placeholder / aria-label  
- `newConn.noDriversMatch` — 空态  

## 测试

- 单元：`getDriverIconMap` 在含 mongodb 等驱动的 generated 下暴露 `db.mongodb` 等键（可 mock generated）。
- 单元或 RTL：搜索过滤 label/id；空 query 显示全部。
- 手工：macOS Overlay 下侧栏独立滚动，traffic lights 下不透出列表项；各类型显示品牌角标。

## 成功标准

- 新建连接左侧每个驱动显示品牌角标（有驱动 SVG 时），不再是统一圆柱图标。
- 左侧列表可独立滚动；整窗不因驱动数量而垂直滚动。
- traffic lights 背后不出现列表文字/图标。
- 可通过 placeholder 搜索框按名称或类型 id 过滤驱动。

## 实现计划

见文首 [实现计划](../plans/2026-08-09-new-connection-driver-icons-ui.md)。
