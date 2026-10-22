# Wave 7 跨轨台账

> 开发期产物，**交付时必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。
> 本文件只记**跨轨**事实；单轨进度留在各轨自己的 `progress.md`。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `count-frontend` | `feature/count-frontend` | `.worktrees/datazen-count-fe` | Coder 交付 5 提交（HEAD `3b23515b0`），**复测中**（Tester `5ef1b3c8`） |
| B `named-views` | `feature/named-views` | `.worktrees/datazen-named-views` | **实现中**（Coder `29f87132`，08 §3.7/§3.8/§4.6 + Step 12–15） |

集成分支 `feature/data-browsing-prd` HEAD = `8fa1d35e3`（Wave 6 已收尾：hub.md 已删）。

## 本波的两条决策（协调者裁，优先级高于分册细节）

### ① `grid.count.estimateUnavailable` **不是错误码**

它描述的是**前端能力位**（用户要了估算 + 面板已加载 + 答案不是估算），不是后端拒绝。⇒ 走错误码通道是错的，且 `PRODUCER_PENDING` 会变成垃圾桶（它永远不会有生产者）。改为：`Pagination` 新增 `estimateUnavailable` prop **直出**文案；`src/lib/gridErrors.ts` 的联合类型与注册表各删一行；`PRODUCER_PENDING` 去掉该条。

代价（诚实记录）：后端真的因为「驱动不支持估算」而拒绝时，前端不再有专属分类 ⇒ 落回原始消息展示。这可接受，因为后端在有生效筛选等场景已经**降级为精确**而非拒绝。

### ② 命名视图的落地形态：**共享规则进 feature-core，持久化留 host**

用户定则要求「两端共同逻辑放共享 crate，`src-tauri` 只做 command 适配」。08 §3.7 的 T0-12 说「持久化留 host，web 侧 desktop-only，迁移存储端口是独立契约改动」。

⇒ 本波把「独立契约改动」**做掉**：
- `packages/feature-core/src/services/named_views/`：DTO + 与形态无关的规则（名称归一 / scope key / 导入重名解析 / validate）+ **存储端口 trait** + domain service；
- `src-tauri/src/store/named_views.rs`：桌面 `Store` 上的端口实现（只做持久化）；
- `src-tauri/src/commands/named_views.rs`：薄壳（校验 → 转发 → 错误映射）；
- web 侧当前无可写 `{appData}` ⇒ 仍须**明确的能力缺失**而不是空列表；`MethodMap` / `BackendClient` / 前端封装 Wave 2 已铺好，本波只把桌面侧注册好并从 `DESKTOP_UNSUPPORTED_METHODS` 删三条。

## 本波已知的形状隐患（Coder 必须处理）

1. **同一个 DTO 两种形状**：`packages/backend-client/src/types/tableData.ts:227-231` 的 `FilterCondition` **没有 `connector`**，而 `src/types/filters.ts:88-101` 与 Rust `packages/driver-api/src/filters.rs:25-35` **都有** ⇒ 保存视图会**丢逐条连接符**。⇒ 只留一份权威 TS 定义 + 逐字相等断言。
2. **`updatedAt` 形状冲突**：TS 是 `string`（必填），Rust 是 `Option<DateTime<Utc>>` ⇒ serde 发 `null`。两侧必须一致。
3. **桌面错误码可达性**：`CommandError::Serialize` 发纯字符串 ⇒ 前缀会丢；前端必须用 `src/lib/fkNavigationActions.ts:90-112` 的 `hostErrorMessage` 沿 `cause` 链取原文，**不能**直接用 `error.message`。

## 跨轨债务台账（Wave 7 之后仍未闭环）

| # | 债务 | 影响 | 归属 |
| --- | --- | --- | --- |
| G1 | 08 Step 10 前端不再剥离不完整条件 ⇔ 宿主必须同时拒绝 | 两边不同步 ⇒ 查询静默变宽 | 08 已成对落地（轨 B + 轨 A），**本波可销** |
| G2 | 09 后端三态 ⇔ 前端三态渲染「≈」 | 后端先上 ⇒ 估算与精确同形 | 本波（轨 A）落地，**本波可销** |
| G3 | 五处越线文件（`traits.rs` 1275 / `reuse.rs` 1484 / `mock_driver.rs` 1091 / `types.rs` 991 / `src/types/index.ts` 707） | 已回落，仍需盯 | 持续 |
| G4 | sqlserver 的 `DATABASE_DRIVER_TRAITS_CEILING` 钉子 | 已修 | 已销 |
| G5 | 05 的 `cellTypes.ts` 未落地（算子按 `DataTypeFamily` 7 族兜底） | 05 落地后只改 `src/lib/filterOperators.ts` 一处 | Wave 10 |
| G6 | 命名视图无人认领 | — | **本波轨 B** |
| G7 | `grid.view.*` 两码未登记 | 门禁 `registered ⊆ produced ∪ PENDING` | **本波轨 B** |
| G8 | `grid.selection.*` 三码零生产者（01 实装走的是 `clampCellCoord → null`，从不抛错） | 挂在 `PRODUCER_PENDING` 里三轮 ⇒ 例外清单变垃圾桶 | **待 Wave 8（02 键盘）后裁定**：键盘导航天然会产生「焦点指向的行已被重取数换掉」的场景，那时 `grid.selection.stale` 可能有真实生产者；**若 02 也不产生它**，则三码全部**撤登记**（注册表里没有生产者的码是对产品错误面的谎报），并同步改契约 00 §8.2 的闭集与两处断言 |
| G9 | i18n 缺口：**192 key × 8 语言 = 1536**（zh-CN 已全；缺的是 de/es/fr/ja/ko/pt-BR/ru/zh-TW）。实测 `node scripts/i18n-sync-check.mjs` EXIT=1，摘要「1536 missing key(s), 0 stale」 | **发布硬门禁**（`scripts/i18n-sync-check.mjs` 在 `.github/workflows/release.yml` 的 `pre-release-guards` 里）。缺口**主要不是本次 PRD 造成的**（chat.shareEditor / newConn.clearStoredCredentials / dashboard.scheduleAuthorization 等是既有欠账），但本 PRD 又新增了 `pagination.*` / `defaultSort.*` / `settings.rowCountMode.*` / `filter.view.*` 等 en key ⇒ 缺口还会变大 | **R 阶段前必须清**；且必须在**全部 en key 定稿之后**做，否则要返工两遍 |
| G10 | 真实 DB E2E 未跑 | 发布信心不足 | R 阶段 |
| G11 | `table_target::read` 与 `get_table_data` 的精确计数解析口径不同 | 改动前就有，登记不改 | 登记 |
| G12 | 估算失败补发无法分辨失败语句 | 可观测性弱 | 登记 |
| G13 | `data/tests.rs` 曾 782/800 | 已拆 | 已销 |
| G14 | Web 通路不持久化三个计数设置 | 需 platform-api + backend-client + 桌面映射同改 | 登记 |
| G15 | `SettingsContent.test.tsx` 1354 行既有超限 | 既有债，本波只加 3 行 fixture | 登记 |

## 门禁基线（Wave 6 合流后实测）

- `npx vitest run` ⇒ EXIT=0，693 files / 7197 tests
- `pnpm typecheck` ⇒ EXIT=0，error TS 0 / TS2578 0
- Rust：宿主 1437 / feature-core 428 / driver-api 403 / platform-infra 26 / postgres 215，全绿；`cargo fmt --check` EXIT=0

## 波次总计划（9 波）

| Wave | 内容 | 状态 |
| --- | --- | --- |
| 1 | `grid-selection` + `insert-backend` | ✅ |
| 2 | `grid-cell` + `web-transport` | ✅ |
| 3 | `insert-frontend` | ✅ |
| 4 | `dirty-cells` + `fk-navigation` | ✅ |
| 5 | `fk-navigation-ui` + `filters` | ✅ |
| 6 | `filters-frontend` + `count-backend` | ✅ |
| 7 | `count-frontend` + `named-views` | 🔄 进行中 |
| 8 | 02 键盘 ∥ 11 键集分页 | ⏳ |
| 9 | 03 剪贴板 ∥ 10 结果网格 | ⏳ |
| 10 | 05 编辑器 ∥ 12 测试 | ⏳ |

R 阶段：全量回归 ⇒ 逐节把 `docs/prd/data-browsing-design/` 改写为「已实现事实」并入 `docs/architecture/` 与 `docs/features/` ⇒ 删除 `docs/prd/` 整个目录 ⇒ 清 G9（i18n）⇒ 推分支。
