# T06 前端接线 — 独立测试报告

- 任务：Parquet 导出 T6（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T6，验收见计划行 62–68；PRD §4 F8 / §5 平台范围矩阵）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`7c3c62fc0` `feat(export): wire the frontend to the Parquet format (T6)`（18 文件，+92/−81）、`0aac5e838` `test(export): pin the Parquet frontend wiring and disable rules (T6)`（5 文件，+268/−177）
- 测试员：独立测试（分工与验证方式同 T02–T05；数字全部来自本机独立运行）。
- 结论：**PASS — T6 可以关闭**。无阻塞缺陷；3 项偏差裁决通过（见 §5）；T02 报告 §5.3 附条件（planner 镜像断言）兑现。
- 报告日期：2026-11-12；验证前后 worktree clean；HEAD = 远端 HEAD = `0aac5e838`。

## 1. 环境与命令

- 前端：node 24 / vitest 4.1.10 / tsc 7.0.2（`scripts/typecheck.mjs --engine=tsc` 权威脚本），依赖按 `pnpm-lock.yaml` 安装（xlsx-0.20.3.tgz 经低速直取官方 CDN 并 sha512 校验一致，见 T02 §6 O-2 环境注记）。
- T6 未改任何 Rust 代码：Rust 门禁继承 T5 结论（121 / 101 / 72，四组合 check 0 warnings，fmt 干净）；提交时 pre-commit 钩子的 `cargo fmt --all` 实跑无 diff。

## 2. 门控结果（全部独立复跑）

| 检查 | 结果 |
|---|---|
| `node scripts/typecheck.mjs --engine=tsc`（root + scripts + pack-ep） | exit 0，**OK** |
| vitest 定向（10 个相关文件） | **322 passed / 322，0 failed**：exportPlan 33、exportDialogShared（矩阵重导出后含 Parquet 全规则）、BatchExportDialog（选项列表 + web CSV 过滤 + parquet 经 Rust 批量路由）、DataExportDialog / ExportDialog（选项、禁用原因顺序、CSV 回退、桌面整表）、exportFormat、batchExportJob、locales（键奇偶校验）、dataFetcher + blobTablesExporter（provided 路径 82 用例，守卫不改变既有格式行为） |
| Rust 侧 | 未触碰；继承 T05 报告 §2/§3 全部数字 |

## 3. 接线核对（diff 全读 + 运行验证）

- `exportPlan.ts`：`formatExtension('parquet') === 'parquet'`、`isBinaryFormat` 加 parquet，"today: XLSX only" 注释更新为二进制双格式——与 Rust `DataFormat::extension` / `is_binary` 镜像逐项一致。
- `ExportTextFormat = Exclude<ExportFormat, 'xlsx' | 'parquet'>`：parquet 不可能进 provided 文本渲染器（tsc 权威证明：初次跑出 dataFetcher:170 与 blobTablesExporter:299 两处收窄错误，正是该排除 + 守卫修复的对象）。
- 选项接线：`SHARED_`（ExportDialog 用）、`ARRAY_`（DataExportDialog 用）与批量对话框 `DATA_FORMAT_OPTIONS` 三处全列 Parquet；`BatchExportDataFormat` / `ExportFormat` 类型并入。
- 禁用规则：`buildExportFormatOptions` 中 Parquet 仅在桌面流式（SQL）范围可用；provided 范围与 Web 全范围灰显并附新原因键 `export.formatDisabledParquetRustOnly`（不隐藏、不静默重置）；XLSX 永不禁用规则未动；SQL UPDATE 无主键规则未动。
- 语言包：新键在全部 10 个 locale（en/zh-CN/zh-TW/ja/ko/de/fr/es/ru/pt-BR）落翻译，`locales.test.ts` 键奇偶校验通过。

## 4. 对抗探针（独立执行）

| # | 探针 | 结果 |
|---|---|---|
| P1 | 平台矩阵（PRD §5）全组合 | **通过**：16 个（scope × platform × 选项表）组合逐一断言——桌面 SQL 范围可用，桌面 provided / Web 全范围灰显且永远在列表中，原因键逐一正确 |
| P2 | 禁用原因不静默重置 | **通过**：Web 切换到整表范围时 Parquet 同其余非 CSV 一样灰显在列，选项不消失；原因顺序按选项顺序 [webSqlScope, parquetRustOnly, noKey] 稳定可断言 |
| P3 | 批量对话框 Web 规则 | **通过**：Web 仅 CSV 的既有过滤把 Parquet 一并排除（与 XLSX 同规则），断言 `['csv']` 不变 |
| P4 | Parquet 真路由 | **通过**：桌面批量对话框选 Parquet → `exportTablesStream` 收到 `dataFormat: 'parquet'`（Rust 批量路径，T5 的分发已在对面等它） |
| P5 | 镜像断言（T02 §5.3 递延项） | **通过**：`formatExtension` / `isBinaryFormat` / 非 SQL 格式 + 多表 zip 判定（`users.parquet` / `orders.parquet` → `useZip: true`） |
| P6 | provided 路径不可走私 | **通过**：`dataFetcher` / `blobTablesExporter` 对走私的 parquet 请求响亮报错（tsc 类型收窄 + 运行时守卫双保险）；82 个 provided 用例证明既有格式行为分毫未变 |
| P7 | tsc 权威 | **通过**：root + scripts + pack-ep 全部 exit 0 |

## 5. 偏差裁决（3 项）

### 5.1 偏差 1：对话框文件路径与计划文档不符——通过（环境勘误）

计划写 `components/export/BatchExportDialog.tsx`；本树中该对话框在 `windows/connection/BatchExportDialog.tsx`，禁用机制在共享的 `lib/exportDialogShared.ts`（DataExportDialog / ExportDialog 经 `buildExportFormatOptions`）。改动落在真实位置，语义与计划逐项对应。裁决：**通过**。

### 5.2 偏差 2：`ExportTextFormat` 排除 parquet + provided 路径走私守卫——通过（tsc 强制的必要扩展）

给 `ExportFormat` 并入 parquet 后，`Exclude<ExportFormat, 'xlsx'>` 会让 parquet 以"文本格式"身份类型可达 provided 渲染器——这是"不碰 provided 导出"红线的反面。实现：类型层排除（`'xlsx' | 'parquet'`）+ 两处运行时守卫（走私报错）。对既有格式零行为变化（82 用例 + 全套件绿佐证）。裁决：**通过**。

### 5.3 偏差 3：批量对话框 Web 端沿用其既有"仅 CSV 过滤"而非灰显——通过

PRD F8 的"禁用并附原因"针对 `buildExportFormatOptions` 体系（DataExportDialog / ExportDialog，已照做）；批量对话框对 Web 的既有设计是过滤到 CSV（XLSX 同样被过滤），Parquet 遵循同一行内规则，不另造新机制。PRD §5"Web 批量 = 不支持"的语义达成。裁决：**通过**，语义差异在 PR 描述中写明。

## 6. 缺陷与非缺陷观察

- **O-1：** 提交 diff 中 feat 的 −81 行主要是 pre-commit prettier 对手改行的规范化重排（92/−81 净增），无语义变化（vitest/tsc 复跑佐证）。
- **O-2：** `parquet` 未加入 `buildProvidedExportFilename` 奇偶校验用例的格式列表——provided Parquet 不存在（PRD 非目标），加进去反而要断言一个不存在的路径。有意为之。
- **O-3：** Rust 侧无改动，Rust 覆盖率 / 测试计数随 T05 维持；前端覆盖率门禁按计划在 T8 一并测。

## 7. 结论

**PASS — T6 可以关闭。**

- 三条线全部接线：planner 镜像（T02 递延项兑现）、批量对话框选项 + 真路由证明、范围对话框的灰显规则 + 十语言原因文案；平台范围矩阵 16 组合全测。
- vitest 322/322、tsc 权威 exit 0、Rust 侧未触碰（门禁继承 T5）。
- 递延项：多表 zip / 类型冲突的 Rust 边界与 pyarrow / DuckDB 读回随 T7；vitest 全套件、前端覆盖率、tsc 全量随 T8。
