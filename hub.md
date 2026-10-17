# hub — P6 AI 生命周期迁移（协调台账，验收合并时删除）

基线 `57ceffc9d`（= feature/p6-remaining）。集成分支 `feature/p6-ai-migration`。

## 已实测基线（集成树，57ceffc9d）
- `npx vitest run` → **608 files / 6413 tests passed**
- `cargo test -p datazen-feature-core --lib` → **261 passed**

## 已提交（集成分支）
- `50976868c` hub 台账
- `bde90ea50` feat(ai): bind AI tool execution to explicit owner lifecycle
  - feature-core：`ai/ownership.rs`（AiTargetClaim / AiGrantReceipt / AiGrantRejection / AiGrantRegistry，时间注入、TTL touch、revoke、revoke_for_editor_session、purge）
  - feature-core：`ai/cancel.rs` owner 指纹 + `CancelReceipt{marked|alreadyFinished|forbidden}` + cancel_owner
  - feature-core：`ai/tool_loop.rs` 取消终态 `done && cancelled`，流式轮次后与每次工具派发前检查取消
  - host：`commands/ai/grant.rs`（AiGrantService + 3 个 IPC + `AiRequestHost` 按 claim 分流、attribution）
  - host：ai_chat/ai_generate_sql 增 `targetClaim`；ai_cancel 返回回执；AppState 增 ai_grants
- `f7724e9cb` docs(ai): 生命周期事实化

## 门禁实测（bde90ea50 后）
- `cargo check -p datazen --all-targets` → **EXIT=0**
- `cargo test -p datazen-feature-core --lib` → **272 passed**（基线 261，+11）
- `cargo test -p datazen --lib` → **1367 passed / 6 ignored / 0 failed**，EXIT=0
- clippy：进行中（job bash-b59546a4bf0e49d3）

## 前端轨
- worktree `.worktrees/datazen-p6-ai-frontend`（feature/p6-ai-frontend），agent 29789d0c 运行中。
- 契约：grant 入参含 `sessionHandle{dbSessionId,runtimeEpoch}`；其余见 ai.md §1.12 六。

## 待办
1. 前端轨返回后合入集成分支，跑 vitest + typecheck。
2. 全新 tester（独立 --detach 工作树）验收：契约对拍、CM-53 H 旅程、拒绝路径、变异检查。
3. F 层旅程（e2e/specs/ai-grant-lifecycle-journey.ts）：尚无，列为遗留。
4. 合回 feature/p6-remaining 需用户确认。

## 2026-10-17 追加
- 合并前端轨（6c50d82af）→ be8701ade；合并后实测 vitest 612 files / 6454 tests EXIT=0，typecheck 四步 EXIT=0。
- a54ba9b64：lint 收口（claim 版 9 参 + dead_code 兼容层）。
- 0c5bee15c：grant 身份绑定修正（不再从指纹反解 client id；执行改用 begin_owned_call）。
- 已知 flaky：mcp::server::tests::mcp_tool_handlers_with_mock_driver（Accepted/Unconfirmed），单跑通过；待 tester 判定是否既有。
- tester agent da485a67 在 0c5bee15c 上验收中。

## 最新状态（tester 验收中）
- HEAD 9cc1e963c：新增取消终态回归测试（ai::tool_loop 2 例），feature-core 274 passed。
- 其余门禁（合并树 be8701ade 后实测）：vitest 612/6454 EXIT=0；typecheck 四步 EXIT=0；host 1367 passed/6 ignored；clippy EXIT=0 且新文件零 warning；check-module-layers ok（1 既有 advisory）；check-platform-crate-boundaries PASS。
- i18n-sync-check：基线 p6-remaining 本就缺 936 键（EXIT=1），本轨新增 4 键 × 8 语言 = +32（合计 968）；发布前须跑 i18n-sync，本轮遵守"开发期只改 en.ts"。
- F 层 E2E 旅程未写（需 webdriver 构建 + 真实 provider 或 mock 通道），列为遗留。
