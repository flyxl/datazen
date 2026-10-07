# 进度台账 — p5-job-addressable

交付即销毁。缺陷结论已变成代码与测试，本文件不是交付物。

- 分支：`feature/p5-job-addressable`
- 基线：`d6423f9d1b6a65d6e3e7a60e54513a7ec0614d67`（`codex/p5-integration` 开工时的 HEAD）
- 提交：`1e0c2132d` 实现，`fe39150b6` 修 `clone_on_copy`
- 门禁跑在 `fe39150b6`（分离工作树 `/tmp/p5ja-verify`，`git worktree add --detach`）
- `CARGO_TARGET_DIR=/tmp/p5ja-target`（本轨独占）

## 三个缺陷与落点

| 缺陷 | 结论 | 落点 |
|---|---|---|
| D1 apply 返回前阻塞，Job 结构性不可寻址 | 已修 | `job_api/mod.rs` `apply_data_transfer_job` → `apply_detached`：受理即返回 jobId，写入 `tauri::async_runtime::spawn` |
| D2 `client.ts` 有 `getJob`/`listJobs`，宿主无对应命令 | 已修 | 新增 `job_api/queries.rs`，`bootstrap/run.rs` 注册 `get_job`/`list_jobs` |
| D3 receipt 查找排在 `claim_plan` 之后 | 已修 | `job_api/mod.rs` `admit_apply`：receipt 查找提到 `admit_apply_plan` 与 `claim_plan` 之前 |

D2 只动宿主命令。前端 `client.ts`、`tauriBackendTransport.ts`、`src/**` 一行未改；opaque filter
由全 `Option` 扁平签名结构化吸收（多余键被忽略、`Option` 缺键可接受）。

## 已完成

- D1/D2/D3 修复 + 12 个新测试（`tests/addressability.rs` 5、`tests/queries.rs` 4、`tests/replay.rs` 3）
- 注释纪律：父会话说「5 处」，实测 `mod.rs` 6 处 `§`、`job_api/**` 共 29 处 `§`，另 6 处缺陷号
  （`D1`/`D2`/`D8`）引用。全部改写为自足理由句。`§` 归零，`\bD[0-9]+\b` 归零。
- `tests/mod.rs` 共享 fixture 补齐 resumable 写路径的三个端点前置条件（有序主键 `IndexInfo`、
  `supports_consistent_snapshot=true`、`empty_keyset_after_cursor=true`）。缺第三个时 keyset 分页
  永不完全，测试挂死（实测 600 s 被杀）；缺前两个时 9 个测试红。

## 进行中 / 未开始

无。全部验证项已跑完并落在上面的门禁里。

## 门禁（`fe39150b6`，分离工作树）

工作树身份首尾各采一次，均为
`HEAD=fe39150b605c8183157740538b5ec73a4d6585fb` / `tree=85dda2f24b7d0bd268294802ccf96bbc7afd0c61` /
`porcelain=0`。提交方工作树全程未被门禁触碰，`main` 工作树 porcelain 0 未动。

| # | 命令 | 退出码 | 逐字结论行 |
|---|---|---|---|
| 1 | `cargo test -p datazen --lib` | 0 | `test result: ok. 1786 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 14.45s` |
| 2 | `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 448 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| 3 | `cargo test -p datazen-data-transfer --lib` | 0 | `test result: ok. 213 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.37s` |
| 5 | `rustfmt --edition 2021 --check <14 个改动文件>` | 0 | 14/14 `OK`，`RUSTFMT_OVERALL=0` |

基线对照：1) 1774 → 1786（净增 12）；2) 448 → 448；3) 213 → 213。

### 门禁 4：clippy 警告集差分

`cargo clippy --workspace --all-targets --keep-going --message-format=json`，取
`level=="warning"` 且 `span.is_primary` 的 `(file_name, lint_code)` 集合。

**两侧用同一个提取脚本**（`/tmp/p5ja-extract.py`）。第一版基线是用另一套脚本生成的
（只存文件名、251 行），与本轨输出逐条 `comm` 会得到 339 条假 ADD —— 已作废重取。

| 指标 | 基线 `d6423f9d` | 本轨 `fe39150b6` | 差 |
|---|---|---|---|
| `(file, code)` 对 | 339 | 339 | ADD 0 / REM 0 |
| warning 消息数 | 1051 | 1051 | 0 |
| primary span 数 | 1107 | 1107 | 0 |
| 不同文件数 | 251 | 251 | 0 |

`CLIPPY_EXIT=101`，**两侧都是 101**，非本轨引入。3 个 error 级诊断均在范围外且两侧逐字相同：

- `packages/drivers/redis/src/ops/exec.rs:260:57` approximate value of `f{32, 64}::consts::PI` found
- `packages/drivers/redis/src/ops/mod.rs:235:38` 同上
- `packages/platform-api/src/error.rs:396:15` non-exhaustive patterns on `variant_name(&PortError)`

负读非空证明：primary-span 路径分布逐条可数（`schema_diff.rs` 76、
`packages/runtime/tests/gateway_fixtures/mod.rs` 42、`commands/sync/host/state.rs` 42 …），
不使用 `Checking <crate> v` 出现次数。`job_api/` 下唯一带警告的文件是
`runtime.rs::clippy::clone_on_copy`，两侧各 2 次（lib + lib-test 各编译一次）。

## 变异矩阵

判定标准：反向变异（把新接线改回旧形状）必须让测试红；不编译的变异不算 kill；存活变异如实记为
未覆盖。基线字面 checkout 连编译都过不了新测试（`job_api/queries.rs` 在基线不存在），所以
「红在修复前」由「每个 mutant 就是旧形状、且 RED」兑现，编译层缺失只作方向证据、不计入 kill。

| ID | 变异 | 结果 | 击杀测试 |
|---|---|---|---|
| D1-A v1 | `spawn` 之前加 `drive.finish().await` | 🗑 **作废** | `error[E0382] use of moved value: drive`，非编译变异，按规则不计 kill（`/tmp/p5ja-mut-D1A.log`） |
| D1-A v2 | 整个 `spawn(...)` 换成 `drive.finish().await`（旧形状） | ✅ KILL | `addressability::a_detached_apply_finishes_on_its_own_and_becomes_readable`，`46 passed; 1 failed` |
| D1-B | `std::mem::drop(drive); Ok(admitted.view)` | ✅ KILL | 同上，`finished in 30.32s`（30 s 有界轮询到期即失败检测器） |
| D1-C | `request_cancel(...).map(\|_\| true)` → `let _ = (&ctx,&id); Ok(true)` | ✅ KILL | `addressability::an_apply_job_can_be_cancelled_through_the_id_it_already_published` + `job_lifecycle::a_cancel_request_lands_on_the_job_repository`，`45 passed; 2 failed` |
| D2-R | 从 `run.rs` 删掉 `get_job`/`list_jobs` 注册 | ✅ KILL | `bootstrap::tests::every_tauri_command_is_registered_and_resolvable`：`these #[tauri::command]s are not registered in bootstrap/run.rs, so IPC fails with "command not found" at runtime: ["get_job", "list_jobs"]` |
| D2-A | 读侧不合并宿主 progress | ✅ KILL（两阶段，见下） | 阶段二 `queries::list_jobs_narrows_by_state_and_by_kind`，`left: JobProgress { read: Counter(0), converted: Counter(0), attempted: Counter(0), committed: Counter(0), unknown: Counter(0) }` |
| D2-B | `JobQuery::matches` 恒返回 `true` | ✅ KILL | `queries::list_jobs_narrows_by_state_and_by_kind` |
| D2-C | 删掉 limit 截断 | ✅ KILL | `queries::list_jobs_applies_a_limit_the_repository_never_enforces` |
| D2-D | 关掉宿主 artifact 合并 | ✅ KILL | `queries::a_sql_file_job_publishes_its_artifact_through_the_read_side` |
| D3-A | receipt 块挪到 `claim_plan` 正后方 | ✅ KILL | `replay::a_repeated_idempotency_key_replays_the_recorded_job` + `replay::a_replay_is_decided_before_the_plan_is_claimed_again`，两者都红在 `Validation("plan was already consumed; re-prepare to review again")` —— 失败文本就是缺陷 3 的症状本身 |
| D3-B | 直接删掉 receipt 块 | ✅ KILL | 同上两个，失败文本相同 |

**D2-A 的两阶段（两次都记）**：阶段一 `job_api::tests::queries` 4 个全绿（EXIT=0）——这是 D2 测试
文件内部的覆盖缺口，由 `addressability` 的轮询测试杀掉。补上断言后阶段二 `queries` 自身转红。

变异矩阵测于格式化前的代码；之后所有编辑只有空白与末尾换行，由格式化后的两次全绿复跑
（`job_api::tests` EXIT=0、完整 `datazen --lib` EXIT=0）证明不改行为。

## 本轨推翻的自己的说法

1. 「注释引用文档章节号 5 处」——实测 `mod.rs` 6 处、`job_api/**` 29 处，另加 6 处缺陷号引用。
   父会话给的数量偏小，按实测值执行。
2. 「`host_progress` 存在即中间进度可观测」——**推翻**。仓库没有 progress 写口
   （`JobView::progress` 只在 `accept` 赋一次 `Default::default()`），`dispatch` 只在结束时把计数
   放进 `JobResult` 返回。宿主镜像**只在 run 结束后**可读，run 进行中调 `getJob` 仍然是零。
   这是范围外契约缺口的范围内绕行，不是修复。
3. 「SQL-file job 的 progress 应当非零」——**推翻**。`packages/data-transfer/src/job/handler.rs`
   的 `run_sql_file` 硬编码 `JobProgress::default()`，emit-only 作业本来就是零。断言改成
   `assert_eq!` 并附理由。
4. 「第一版基线警告集可以直接 comm」——**推翻**。两侧提取脚本形状不同，第一版得到 339 条假
   ADD。两侧改用同一脚本重取。
5. 「我的改动没有引入任何新警告」——**推翻过一次**。头一次差分确实 ADDS=0/REMS=0，但按消息条数
   复核时 `job_api/runtime.rs` 的 `clone_on_copy` 从 2 次涨到 4 次：集合差分看不见同文件同 lint
   的新增实例。`JobProgress` 派生了 `Copy`，`progress.clone()` 是我写的。改成 `*progress`，
   条数回到 2。**只比对集合是不够的，必须同时比对条数。**

## 遗留

1. **跨包缺陷，本轨不修**：`Counter` 的 `Serialize` 走 `serializer.collect_str(&self.0)`，JSON 里
   计数器是**字符串**；前端 `toCounter`（`packages/backend-client/src/types/identity.ts`）只接受
   `typeof value === 'number'`。于是 `parseJobView` 会把所有 `progress.*` 读成 0。
   `packages/**` 与 `src/**` 都不在本轨范围。实测输出形如
   `{"read":"7","converted":"0",...}`。**因此任何测试都不得把 progress 的字符串形态写死。**
2. **`JobView` 没有 `error` 字段、没有游标**：`JobFilter.after` 无法在宿主侧实现，故**故意留空**并
   在代码注释里写明。`JobFilter` 也没有 `kind`，所以 kind 过滤在宿主侧做。
3. **`InMemoryJobRepository::list` 只认 `states` 和 `owner`**，忽略 `after`/`limit`，且按
   `created_at` 升序 —— 所以 limit 必须在宿主侧兜底，这正是 D2-C 变异要杀的东西。
4. **clippy EXIT=101 是基线既有**，且其中 2 个（redis approx_constant）已被集成分支上
   `f04669bd4`/`366004082` 修掉。本轨的门禁必须在合入集成分支后**重跑一次**再定性。
5. **集成分支已从 `d6423f9d` 前进到 `6fc790a23`**（19 个提交）。与本轨 14 个文件**零重叠**
   （`comm -12` 为空），文本上不会冲突；但仍需常规合并。

## 越界项（明确报备）

- `src-tauri/src/bootstrap/run.rs` **+5 行**：`generate_handler!` 里加 `get_job`/`list_jobs`
  两条注册 + 3 行中文注释。不加则两个命令存在但调不到，D2 不算修完；
  `bootstrap/tests.rs` 的 `every_tauri_command_is_registered_and_resolvable` 也会红。
  **该文件不在本轨范围，属最小必要越界，请集成裁定。**
- `packages/data-transfer/src/job/handler.rs` 曾被探针改动，**已完全回滚**，不出现在最终 diff 里。
- `.env` / `.env.test` 内容未读取。`pnpm install` 未执行。`cargo fmt` 未执行。
  `capabilities/default.json` 与 `driver_init.rs` 均为 gitignore 的 codegen 产物，只复制未编辑。