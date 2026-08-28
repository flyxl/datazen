# Schema Diff UI 重构（方案 B）— 进度台账

> 协调者维护总览与 bug 台账；各轨道代理只写本功能小节。  
> 规格：[schema-diff-ui-redesign.zh-CN.md](../features/schema-diff-ui-redesign.zh-CN.md) §4 方案 B

## 1. 功能总览

| 编号 | 功能 | 轨道 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------|-------------|-------------|
| F1 | EndpointsBar + dedicated db/schema 会话 | schema-diff-ui-endpoints | 编码中 | — | — |
| F2 | 双栏面板（左表/diff · 右 plan/deploy） | schema-diff-ui-panels | 编码中 | — | — |
| F3 | SchemaDiffWindow 集成 + bg-surface shell | schema-diff-ui-b | 未开始 | — | — |
| F4 | Host 单测 | schema-diff-ui-tests | 未开始 | — | — |
| F5 | E2E 适配 | schema-diff-ui-e2e | 未开始 | — | — |

## 2. Bug 台账

| Bug ID | 所属 | 描述 | 状态 | 记录时间 |
|--------|------|------|------|----------|
| — | — | — | — | — |

## 3. 测试约定

- Host 单测：`npx vitest run src/windows/schema-diff`
- E2E 套件：`pnpm e2e:schema-diff`（R 阶段统一 `--skip-build` 或 `:build`）
- 方案 B 验收：Sync 式双栏；EndpointsBar 常驻；Deploy 在右栏/抽屉；保留 DEPLOY token

## 4. 功能小节

### F1 schema-diff-ui-endpoints

- **范围**：`SchemaDiffEndpointsBar.tsx`、`useSchemaDiffEndpoints.ts`（新文件，勿改 `SchemaDiffWindow.tsx`）
- **验收**：
  - [ ] 源/目标连接 + database + schema（有则显示）Select
  - [ ] Swap 按钮交换源/目标
  - [ ] Compare 主按钮在 bar 内
  - [ ] testid：`schema-diff-source`、`schema-diff-target`、`*-database`、`*-schema`
  - [ ] 复用 Sync EndpointsBar 视觉 token（`border-edge px-6 py-4`）

### F2 schema-diff-ui-panels

- **范围**：`SchemaDiffTableListPanel.tsx`、`SchemaDiffRightPanel.tsx`、`SchemaDiffDeployDrawer.tsx`（新文件）
- **验收**：
  - [ ] 左栏：表列表 + 选中高亮 + diff 摘要/badge
  - [ ] 右栏：Plan tab（SQL 列表 + options）+ Deploy 区（含 DEPLOY 输入）
  - [ ] Props 驱动，无 window 状态耦合
  - [ ] testid 覆盖 list、plan、deploy

### F3 schema-diff-ui-b（集成）

- **范围**：`SchemaDiffWindow.tsx` 重构为 Sync 式布局；接入 F1/F2 组件
- **验收**：
  - [ ] `bg-surface`；无双栏滚动堆叠旧 UI
  - [ ] Limitations 弹窗仍可用
  - [ ] 行为回归：compare → plan → deploy 闭环

### F4 / F5 测试轨

- 编码合并后启动；E2E 用例变更登记于本文件
