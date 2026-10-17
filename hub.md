# hub — P6 AI 生命周期迁移（协调台账，验收合并时删除）

主检出 `/Users/wuxiaolong/code/rust-projects/datazen` 仅用于合流。基线：`57ceffc9d`（= feature/p6-remaining）。集成分支：`feature/p6-ai-migration`（worktree .worktrees/datazen-p6-ai-migration，已铺装）。

## Wave 1（进行中，2026-10-17 派发）
- **backend** `feature/p6-ai-backend` @ .worktrees/datazen-p6-ai-backend — agent 34a40e43-338a-44cd-a870-67ce41d9aa70
- **frontend** `feature/p6-ai-frontend` @ .worktrees/datazen-p6-ai-frontend — agent efdc3483-705f-4353-a04b-875e3f54375d
- 文件冲突面互斥：backend = src-tauri/** + packages/feature-core/**；frontend = src/**。CARGO_TARGET_DIR 各轨独立（/tmp/dz-p6-ai-backend-target）。

## 冻结契约（两轨逐字实现；合流后 tester 对拍）
- AiTargetClaim = Independent | EditorGrant{grantId, claimedClientInstanceId, editorSessionId}（serde tag kind, camelCase）
- AiGrantReceipt{grantId, connectionId, database|null, configRevision, expiresAtMs}
- CancelReceipt = marked | alreadyFinished | forbidden{requestId}；ai_cancel 返回类型 bool→CancelReceipt
- IPC：ai_grant_editor_session / ai_revoke_editor_grant / ai_touch_editor_grant；ai_chat 等增 targetClaim?
- 工具结果 attribution{grantId?, executionId?, ownerKind:"independent"|"editorGrant", connectionId, database, configRevision}
- 取消终态 chunk done:true && cancelled:true（修 tool_loop 现状 cancelled:false）
- 默认 = independent；EditorGrant 拒绝时禁止静默降级（不得双模回退）
- grant registry：feature-core 内可移植、时间注入、TTL 默认 15min；owner 指纹全字段核验

## 范围裁定
- 本轮只做 **AI**（用户要求「完成 AI 功能的迁移」）。MCP/Wapp 生命周期不在本轨；ConnectionManager 不删；PG+MySQL only。
- 基线已把 AI 领域逻辑迁入 packages/feature-core/src/ai/*（零 tauri）、ai-api/ai-providers 已解宿主化——本轨交付的是 §14.4 的运行时生命周期：owner 授权三态、取消回执、资源回收（面板卸载/切 pane/grant 撤销/editor 关闭联动）、结果归属。

## Wave 2（Wave 1 合流后）
- integration worktree 合两轨 → 修契约对拍偏差 → F 层旅程（e2e/specs/ai-grant-lifecycle-journey.ts，用 __invokeCalls stub 通道断言 IPC 载荷 + 真实 UI 状态机；服务端拒绝路径属 H 已有）→ **全新 tester**（独立 --detach 工作树）→ 缺陷循环（≤5 轮）→ 合回 feature/p6-remaining 需用户确认。

## 门禁基线（合流后跑）
npx vitest run / pnpm typecheck / cargo test -p datazen --lib / cargo test -p datazen-feature-core --lib / cargo clippy（新代码零 warning）/ node scripts/check-frontend-architecture.mjs / cargo fmt --check。输出落系统临时文件取结论行。
