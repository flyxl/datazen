# Brief: sql-s7 — 端到端收口与性能硬化（顺序执行 S7-A → S7-B → S7-C）

> 依赖：S6-D 已合流。本阶段三轨顺序执行（共改主 E2E spec / coverage 文档 / 装配文件）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Stage 7 + §6/§7/§9（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s7-*/progress.md`。

## S7-A：中央装配审计（Tester-only，只记录不修）

1. 核对 SqlEditor、QueryPanel、ContentView、schema tree 的 props 和生命周期。
2. 搜索旧重复 scanner、绕开 query-submit gate 的入口、直接 IPC completion；
   只记录 Bug（ObjectBrowser/Import/Privilege/DDL/导出等非 QueryPanel submit 路径不算绕过）。
3. 检查新增及实质修改的生产 TS/Rust 文件行数、`any`、硬编码 driver ID、错误 connection/session ID。
4. 检查 listener/tooltip/debounce/cache 在 unmount、session 切换、文档切换时清理。
5. 检查无 dbx import/复制结构/运行时依赖。
6. 发现问题按原轨归属写 bugs，恢复原 Coder 修复 + 新独立审计 Tester；本轨不得"顺手清理"。

## S7-B：Host E2E（运行/去抖已归属 spec + 一个跨功能 journey）

各功能 Tester 应在合流前提交运行自己拥有的 spec；S7-B 只运行/去抖/补跨功能 journey：

- `sql-editor-statement.ts`：多语句 frame、gutter、shortcut、running、多结果回归。
- `sql-editor-intelligence.ts`：alias completion、FK join、Alt+Enter、hint、hover/navigation。
- `sql-editor-productivity.ts`：Paste as IN、table/column drop、Mod+D。
- `sql-editor-safety-params.ts`：参数 journey、历史、Safe Mode、production confirmation。
- `sql-editor-ai-error.ts`：确定性错误、Ask in Chat 草稿、脱敏（不调真实 LLM）。

要求：Host 通用行为断言（方言引号 E2E 放驱动目录）；每测自建自清（表/分组/Safe Mode 状态，
失败也恢复）；新增 journey 同步 `docs/development/e2e-coverage.md`；不可自动化登记例外
（原因+手工步骤+owner）。构建必须用 `pnpm tauri:build:webdriver`（禁裸 tauri build）。

## S7-C：性能与降级验证

1. 固定 fixture 测：首次 scanner、活动 statement parse、单字符编辑、selection move、scroll、completion source。
2. warmup 后 ≥30 次，记 median/p95；`<5ms` = 活动 statement semantic parse p95（非首次整文档扫描）。
3. 20k 文档 cursor/selection 移动不得全文重扫；单字符编辑只重算受影响 window/range。
4. Performance trace：持续输入/滚动无 >50ms 长任务，目标 60fps。
5. 500+ 行单 statement frame 降级；大 INSERT hint 只处理 viewport；hover 快移请求取消。
6. 普通 CI 宽松阈值；严格 p95/60fps 放固定机器脚本+发布门禁，输出原始数据。
7. 未达标 TEST_FAILED 回流 owner；只有用户/产品书面 waiver 才可标 AC-19 waived（Tester 无权降级）。

## R 阶段全量回归（S7 之后）

1. `pnpm build`（或 `npx tsc --noEmit`）+ `cargo test -p datazen --lib` + 前端单测。
2. 逐项回归所有轨 progress.md 登记的【留待 R 回归】E2E。
3. AC-01~AC-21 逐项给可重复证据；无未关闭 P0/P1/P2 bug 才可标记 PRD 实施完成。
