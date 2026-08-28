# Schema Diff（结构比对）UI 审查报告

> 审查日期：2026-08-28  
> 代码基线：`main` + 本地 WIP（limitations 弹窗、E2E 套件、`schemaDiff` 功能门闸）  
> 关联文档：[schema-diff-guide.zh-CN.md](./schema-diff-guide.zh-CN.md)、[schema-diff-deploy.md](./schema-diff-deploy.md)、[architecture/backend/schema-diff.md](../architecture/backend/schema-diff.md)

---

## 1. 结论摘要

Schema Diff 采用 **单页滚动 + 三步文字面包屑**（Compare → Plan → Review/Deploy），与 Data Sync 的单页工作台、Data Transfer 的 6 步向导均不同。核心能力完整（对比 → 计划 → 受控部署），安全默认值合理（默认仅加法、破坏性需勾选 + 输入 `DEPLOY`）。

**当前状态：**

| 维度 | 评价 |
|------|------|
| 功能完整度 | 高 — 对比/计划/部署闭环可用 |
| 与 Sync/Transfer 视觉一致 | 中 — 背景 token、步骤指示、端点选择器未对齐 |
| 安全 UX | 高 — 多层 gate（allowDestructive、DEPLOY token、rollback 完整性） |
| 可测试性 | 中 → 高（WIP 补充 testid + E2E 套件） |
| 文档一致性 | 中 — 配置 JSON v2、帮助链接等有漂移 |

**建议优先级：** P0 对齐视觉 token + 提交 WIP E2E；P1 补 database/schema 选择器；P2 统一 Deploy 按钮组件、刷新用户手册 JSON 示例。

---

## 2. 入口与可见性

| 入口 | 路径 | 预填 | 门闸 |
|------|------|------|------|
| macOS 系统菜单 Tools → Schema Diff | `menu:schema-diff` | 否 | `productFeatures.schemaDiff`（WIP 改为 `true`） |
| Windows/Linux Web MenuBar | 同上 | 否 | 同上 |
| 连接树右键「比较架构」 | DB/Schema 节点 | 否 | 同上 |
| Data Sync 映射面板 | 「结构对比」按钮 | 否 | 无 |
| 直接 URL（E2E/开发） | `window.html?window=schema-diff` | 否 | 无 |
| TitleBar 帮助 | BookOpen → 文档 | — | 链接当前落到 Sync 章节（`#sync`） |

子窗口：单例 `schema-diff-singleton`，默认约 900×640。

---

## 3. 界面结构

```
SchemaDiffWindow (bg-canvas)
├── TitleBar（标题 + 帮助按钮）
├── 滚动主区 (p-6, 单列)
│   ├── 功能说明 (schemaDiff.description)
│   ├── 步骤面包屑 Compare → Plan → Review
│   ├── 源/目标连接 Select（复用 sync.source/target）
│   ├── 表名 textarea（多行/逗号分隔）
│   ├── 操作栏：对比 | 生成计划 | 复制摘要 | 导出/导入配置
│   ├── 错误行 (inline text-danger)
│   ├── 对比结果卡片 × N → SchemaDiffPanel
│   ├── 计划卡片 → SchemaDiffPlanPanel
│   └── 审阅/部署卡片 → SchemaDiffDeployPanel（step === review 时）
├── SchemaDiffLimitationsDialog（首次打开，WIP）
└── StatusBar
```

### 3.1 关键组件

| 组件 | 职责 |
|------|------|
| `SchemaDiffWindow.tsx` | 状态编排、dedicated session、三步 `Step` |
| `SchemaDiffPlanPanel.tsx` | allowDestructive / includeIndexes、语句列表、风险 badge |
| `SchemaDiffDeployPanel.tsx` | 目标摘要、事务/回滚 gate、`DEPLOY` 输入、部署结果 |
| `SchemaDiffLimitationsDialog.tsx` | 能力限制弹窗（对齐 Transfer 模式，WIP） |
| `components/schema/SchemaDiffPanel.tsx` | 列级 diff 展示（Sync 侧也复用概念） |

---

## 4. 用户流程

**单页滚动，非分页向导。**

```mermaid
flowchart LR
  A[选源/目标 + 填表名] --> B[对比]
  B --> C[生成部署脚本]
  C --> D[审阅/部署]
  D --> E[输入 DEPLOY 若含 destructive/rewrite]
  E --> F[部署到目标]
```

| 步骤 | 用户操作 | UI 行为 |
|------|----------|---------|
| Compare | 选连接、填表、点「对比」 | 每表一张 `SchemaDiffPanel`；可跳过对比直接生成计划 |
| Plan | 「生成部署脚本」 | 展示 SQL 列表 + additive/destructive/rewrite 标记；可勾选选项后「重新生成」 |
| Review | 「审阅 / 部署」 | 计划区仍可见；下方展开 Deploy 面板 |
| Deploy | 勾选事务/回滚要求，必要时输入 `DEPLOY` | 仅在目标库执行；展示 committed/rolled_back/mixed/failed |

**与 Data Sync 差异：** Sync 是顶栏端点 + 表映射 + ExecuteBar 进度；Schema Diff 无映射网格、无执行进度条，部署一次性完成。

**与 Data Transfer 差异：** Transfer 是 6 步编号 stepper + Preview 底栏 Execute；Schema Diff 无独立 Execute 步，破坏性确认是 **输入 token** 而非 Modal。

---

## 5. 与 Sync / Transfer 的设计对齐

| 维度 | Schema Diff | Data Sync | Data Transfer |
|------|-------------|-----------|---------------|
| 页面背景 | `bg-canvas` | `bg-surface` | `bg-surface` |
| 步骤 UX | 文字面包屑 `→` | 状态机 + 面板切换 | 编号 pill stepper |
| 端点选择 | 仅连接 | 连接 + 库 + schema + swap | 连接 + 库 |
| 限制说明 | 弹窗 + localStorage（WIP） | 无 | 弹窗 + localStorage |
| 破坏性确认 | Plan 勾选 + Deploy 输入 `DEPLOY` | Execute/Delete Dialog | Setup 勾选 + Execute Modal |
| 主操作按钮 | 混用 `Button` 与原生 `<button>`（Deploy） | 统一 `Button` / ExecuteBar | 统一 `Button` |
| 帮助入口 | TitleBar BookOpen | 无 | 无 |
| 共享 i18n | `sync.source/target/...` | `sync.*` | `transfer.*` |

**应对齐项（建议）：**

1. 背景改为 `bg-surface`，边框/卡片用 `border-edge` + `bg-surface-alt`（与 Transfer 重构后一致）
2. Deploy 按钮改用 `Button variant="primary|danger"`
3. 步骤指示可升级为轻量 stepper（不必做成 Transfer 级 6 步，但视觉 token 应统一）
4. 限制弹窗 pattern 已与 Transfer 一致，可共用文档说明

---

## 6. 对话框与安全模式

### 6.1 能力限制弹窗（WIP）

- 首次打开显示；可勾选「不再显示此提示」
- localStorage：`datazen:schema-diff-limitations-dismissed`
- 内容要点：无视图/例程、无在线 ALTER、无重命名推断、跨方言 caveat、无自动备份、无 MCP 部署

### 6.2 破坏性部署 gate

| 层级 | 机制 |
|------|------|
| 计划生成 | `allowDestructive` 默认 **关** — 不含 DROP/收窄 |
| 部署执行 | 若任一条为 `destructive` 或 `rewrite`，必须输入 **`DEPLOY`** |
| 回滚完整性 | 可选 `requireRollback` — 缺 rollbackSql 时前端禁用 Deploy（**后端未强制**） |
| 事务 | PG/SQLite 默认开；MySQL 显示不可事务 hint 并禁用 |

错误以 **inline 文本** 展示，无独立 Error Dialog（与 Transfer 不同）。

---

## 7. i18n

- 主命名空间：`schemaDiff.*`（约 43 key，含 WIP 的 `limitations.*`）
- 端点标签复用 `sync.source` / `sync.target` 等
- 遗留未使用 key：`schemaDiff.table` / `schemaDiff.tablePlaceholder`（可清理）
- 其他语言文件暂缺 `limitations.*`（发布前需 `i18n-sync`）

---

## 8. 测试与 E2E 覆盖

### 8.1 单元测试

| 区域 | 文件 | 状态 |
|------|------|------|
| IPC / plan | `commands/__tests__/schemaDiff.test.ts` | ✅ |
| DEPLOY 确认逻辑 | `lib/__tests__/schemaDiffConfirm.test.ts` | ✅ |
| 功能门闸 | `lib/__tests__/productFeatures.test.ts` | ✅（WIP） |
| 窗口组件 | — | ❌ 无 `SchemaDiffWindow.test.tsx` |

### 8.2 E2E（WIP：`pnpm e2e:schema-diff`）

| Spec | 覆盖 |
|------|------|
| `schema-diff-window.ts` | SD-001~004 窗口壳、控件、校验；SD-LIM 限制弹窗 |
| `schema-diff-diverse-types.ts` | 宽类型多表 |
| `schema-diff-cross-dialect.ts` | PG↔MySQL 跨方言 |
| `schema-diff-options-matrix.ts` | allowDestructive / includeIndexes 矩阵 |
| `journeys/schema-diff-journey.ts` | PG→PG 全旅程 |
| `journeys/schema-diff-*-journey.ts` | 跨方言旅程（未跟踪） |

**尚未 E2E 覆盖的 UI 路径：**

- 导出/导入配置 JSON
- 复制摘要 / 复制 SQL
- 重新生成计划
- 菜单/连接树入口（E2E 走 direct URL）
- `requireRollback` 交互
- 输入 `DEPLOY` 的破坏性部署全路径
- TitleBar 帮助按钮

---

## 9. 差距与 Backlog

| # | 项目 | 严重度 | 说明 |
|---|------|--------|------|
| 1 | 无 database/schema 下拉 | 高 | 依赖连接默认库 + 表名限定；多 schema 体验差 |
| 2 | 用户手册 JSON 示例 v1 | 中 | 代码仅接受 `version: 2` + `sourceConnectionId` |
| 3 | 帮助链接 `#sync` | 低 | 应指向 Schema Diff 专章 |
| 4 | `requireRollback` 仅前端 | 中 | 安全 gate 可被绕过 |
| 5 | 无 Schema Diff Vitest | 中 | 对比 Sync/Transfer 测试落点 |
| 6 | Deploy 原生 button | 低 | 视觉/无障碍一致性 |
| 7 | `bg-canvas` vs `bg-surface` | 低 | 三工具视觉割裂 |
| 8 | 无部署前 Modal | 低 | 依赖 token 输入，与 Transfer Execute Modal 策略不同但可接受 |

---

## 10. 本地 WIP 状态（未全部提交）

**已修改（staged/未提交混合）：**

- `SchemaDiffWindow.tsx` — limitations 集成、testid
- `SchemaDiffPlanPanel.tsx` / `SchemaDiffDeployPanel.tsx` — testid
- `SchemaDiffLimitationsDialog.tsx` — 新文件（未跟踪）
- `schemaDiffLimitationKeys.ts` / `schemaDiffLimitationsPrefs.ts` — 新文件
- `productFeatures.ts` — `schemaDiff: true`
- `e2e/specs/schema-diff-window.ts` + 新 E2E spec 文件
- `package.json` — `e2e:schema-diff` 脚本

**建议下一步：**

1. 提交 WIP 并跑通 `pnpm e2e:schema-diff:build`
2. 补 `SchemaDiffWindow.test.tsx`（limitations dismiss + DEPLOY gate）
3. 更新 `schema-diff-guide.zh-CN.md` 的 JSON v2 示例
4. 视觉 token 对齐（可与 Sync 下一版一并做）

---

## 11. 与 Data Transfer UI 重构的对照

| Transfer 已做 | Schema Diff 对应 |
|---------------|------------------|
| 6 步 stepper | 文字面包屑（可升级） |
| Limitations 弹窗 | WIP 已复制 pattern |
| Preview Execute Modal | Deploy 用 DEPLOY token（不同策略，合理） |
| `bg-surface` / `border-edge` | 仍用 `bg-canvas` |
| E2E 全 suite | WIP 中，接近 parity |

Schema Diff **不需要**强行改成 6 步向导；单页 + 三阶段更符合「审阅 SQL 后部署」的心智模型。重点应放在 **视觉 token 统一**、**E2E 补齐**、**端点选择器增强**。
