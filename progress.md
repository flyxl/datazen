# progress.md — Wave-2 Track C3「platform-infra 脱敏下沉」

> 轨道台账。**合并到 main 时必须删除本文件**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

- 轨道分支：`feature/platform-infra`（worktree：`.worktrees/datazen-platform-infra`）
- 基线提交：`179c6c8b67022cbc8f86207c9e6c409b55525075`（协调者预置：注册位 + F-11 规则），本轨道不 amend
- 目标：把日志脱敏从宿主私有模块下沉为形态中立基础设施原语；宿主退化为薄壳；**调用点零改动**

## 已完成

| 任务 | 落点 | 结论 |
| --- | --- | --- |
| T1 | `packages/platform-infra/src/redaction.rs`（140 行） | 实现逐字迁入，`OnceLock` 惰性编译保留，5 个单测随实现同迁 |
| T2 | `src-tauri/src/log_redact.rs`（118 → 8 行） | 只剩 `pub use` 再导出（**非** `pub(crate) use`），`mod log_redact;` 未动 |
| T3 | `src-tauri/Cargo.toml:27-28` | 依赖落在 `# <<driver-dependencies>>` 标记块之外，未用 `{ workspace = true }` |
| T4 | `packages/platform-infra/Cargo.toml` | 仅 `regex` + `thiserror`，零 `datazen-*` 依赖，未新增 tracing/serde |
| T5 | `AGENTS.md:53` | `platform-infra/` 行插在 `platform-api/` 之后，注释列与邻居对齐 |
| T6 | `docs/architecture/platform/shared-boundaries-and-ports.md` | **零改动**：§2.4 F-11 行与 §2.5.1（:185-215）逐条复核，无分歧可校准 |
| T7 | 本文件 | 台账 |

## 门禁实测

在**恢复后的同一棵工作树**上跑（HEAD `179c6c8b6`，工作区 sha `c0fb3138…`）。

| 门禁 | 命令 | 退出码 | 逐字结论行 |
| --- | --- | --- | --- |
| G1 | `cargo build -p datazen-platform-infra` | 0 | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.41s` |
| G2 | `cargo test -p datazen-platform-infra` | 0 | `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` |
| G3 | `cargo build -p datazen` | 0 | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 10.87s` |
| G4 | `cargo test -p datazen --lib` | 0 | `running 1711 tests` / `test result: ok. 1705 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 10.10s` |
| G5 | `pnpm --config.verify-deps-before-run=false test:platform-arch` | 0 | `ok F-11: datazen-platform-infra` / `PASS — 29 workspace member(s) classified, 10 rule(s) evaluated over 31 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 2 advisory(ies)` |
| G6 | `pnpm --config.verify-deps-before-run=false test:platform-crates` | 0 | `[platform-crate-tests] PASS — 10 core crate(s) tested`（含 `datazen-platform-infra`） |
| G7 | `pnpm --config.verify-deps-before-run=false test:layers` | 0 | `[check-module-layers] ok (4 rules, 87 files examined, 1 advisory)` |
| G8 | `pnpm --config.verify-deps-before-run=false test:boundaries` | 0 | `[check-driver-import-boundaries] ok (1788 file(s) scanned · 0 blocking violation(s) · 1 advisory finding(s))` |
| G9 | `pnpm --config.verify-deps-before-run=false test:scripts` | 0 | `Test Files  43 passed (43)` / `Tests  760 passed (760)` |
| G10 | `cargo fmt --all -- --check` | 0 | 输出 0 行（零 diff） |
| G11 | `pnpm --config.verify-deps-before-run=false typecheck` | 0 | `tsc --noEmit` 三条子链全过，无 error TS |
| G12 | `git diff --stat HEAD -- src-tauri/src` | 0 | `src-tauri/src/log_redact.rs | 122 +++---` / `1 file changed, 6 insertions(+), 116 deletions(-)` |
| G13 | `git diff --name-only HEAD~1` | 0 | 见提交后记录 |
| G14 | F-11 反向证明（临时注入 `datazen-platform-api`） | 1 → 0 | 违规：`VIOLATION F-11  datazen-platform-infra (packages/platform-infra/Cargo.toml) → datazen-platform-api in its normal+build closure`；`git checkout` 还原后：`ok F-11: datazen-platform-infra` |

## 测试数算术（正确性证明）

| 范围 | 基线 | 本轨道后 | 差 |
| --- | --- | --- | --- |
| `datazen --lib` | 1716 running / **1710 passed** / 0 failed / 6 ignored | 1711 running / **1705 passed** / 0 failed / 6 ignored | −5 passed |
| `datazen-platform-infra` | 0（当时无测试） | 5 passed / 0 failed / 0 ignored | +5 |

-5 = 迁走的 5 个脱敏单测（`sql_preview_truncates_and_redacts`、`redacts_bearer_and_query_tokens`、`redact_url_strips_query_and_userinfo`、`redacts_mysql_and_mongo_uris`、`redacts_redis_password_only_userinfo`），无一项丢失或重复。1710 未降 → 存在重复实现；1704 → 少迁一个测试。

## 调用点零改动证据

- `grep -rno 'crate::log_redact::[a-z_]*' src-tauri/src --include='*.rs'` → **18** 个真实调用点（另 1 条是 `log_redact.rs:4` 文档注释里的 `crate::log_redact::*` 字样，非调用）。
  - `sql_preview_for_log` 5、`redact_secrets_for_log` 10、`redact_url_for_log` 3。
- `git diff --stat HEAD -- src-tauri/src` 仅 `log_redact.rs` 一个文件：宿主其余 `.rs` 逐字节未动（G12）。

## 遗留与待裁定

1. **`Cargo.lock` 已被本轨道的 path 依赖改动（+37 行），但按纪律不入库。** 它是 git 跟踪文件、非 gitignore，需协调者决定由谁在合并时统一提交。→ **待裁定**
2. **`stash@{0} dzinfra-baseline` 是本轨道为取 G5 基线临时建的，现已无对应内容。** 取基线的 pop 曾失败（工作树回退到 HEAD，靠 `git checkout 'stash@{0}' -- <5 paths>` 按指纹还原）。`.git stash` 是仓库共享状态、多轨可见，故未自行 drop。→ **待裁定（建议协调者 `git stash drop stash@{0}`）**
3. **F-11 报出的 member/rule/crate 计数前后不变（29 / 10 / 31）**：`packages/platform-infra` 的 workspace member 位与 F-11 规则本身已由预置提交 `179c6c8b6` 落下，本轨道只加依赖。简报预估的「层数 29 → 30」未复现，以实测为准。
4. **`advisory 3 → 2` 与本改动无因果关系**：消失的那条是 `datazen-driver-redis declares tauri in [dependencies], which the resolved graph does not contain`，成因是基线跑在 HEAD 版 `Cargo.lock`（无 resolve-drivers 注入项）而复跑跑在工作区版 `Cargo.lock`。
5. **path_safety 未下沉**：`packages/workspace-apps` 在本分支只是 stub（仅 `src/lib.rs`），`MAX_COMPRESSION_RATIO` 按范围留给 Wave-2 合并后处理。
6. 本轨道未触碰 `packages/ai-providers`、未新增 PRD/设计/评审文档。
