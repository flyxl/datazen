# 轨道 A：job-org-scope 进度台账

> 交付后随分支删除（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

## 目标

让 job 的**持久化词汇**带上 `organization_id`，使多租户（web 形态）能按组织隔离 job 的
受理、列举与恢复。桌面是单租户，桌面行为必须完全不变。

## 已完成

| # | 事项 | 落点 | 状态 |
| --- | --- | --- | --- |
| 1 | `JobDefinition` 新增**必填** `organization_id: OrganizationId` | `packages/platform-api/src/dto/job.rs` | 完成 |
| 2 | `JobFilter` 新增**必填** `organization_id: OrganizationId`，并**移除 `Default` 派生**（省略组织维度就没有合法的列举调用） | 同上 | 完成 |
| 3 | 端口契约文档补「组织维度」段；`accept` 盖章、`list`/`get`/`list_recoverable` 按组织过滤、跨组织按「不存在」处理 | `packages/platform-api/src/ports/job.rs` | 完成 |
| 4 | `InMemoryJobRepository`：`accept` 按 ctx 盖章并拒绝重复 jobId；幂等回执按 `(org, principal, key)` 作用域；已消费 planId 按 `(org, planId)` 唯一；`list`/`get`/`list_recoverable`/claim/写路径按组织过滤 | `packages/runtime/src/job/repository.rs` | 完成 |
| 5 | `owner_claims.rs`（行归属 + claim 绑定双重核对组织）、`runtime_methods.rs`（进度/产物/域结果/恢复/详情全部带 ctx 组织核对） | `packages/runtime/src/job/repository/` | 完成 |
| 6 | `JobRuntimeRepository` 文档补组织语义；`memory_runtime_repository.rs` 全量透传 ctx | `packages/runtime/src/job/` | 完成 |
| 7 | SQLite 侧：`read_record` 读出 `organization_id` 填进 `JobDefinition`；`admit_job` 按 ctx 盖章；`list` 校验 `filter.organization_id` | `src-tauri/src/store/app_db/jobs/{codec,admission,access}.rs` | 完成 |
| 8 | 全仓 `JobDefinition` / `JobFilter` 构造点补字段（host / runtime / feature-core / data-transfer / schema-diff / mysql / postgres 驱动测试） | 见 `git show --stat` | 完成 |
| 9 | 文档：端口 §4.3 补「Job 的组织维度（已实现）」 | `docs/architecture/platform/shared-boundaries-and-ports.md` | 完成 |
| 10 | 新增组织隔离单测 15 条（platform-api 3 / runtime 8 / host 4） | 见下 | 完成 |

### SQLite schema 迁移：无需新增

任务书 (e) 假设 `jobs` 表还没有 `organization_id`。**实测：已经有**——`src-tauri/src/store/app_db.rs`
的 `JOB_SCHEMA_V2` 里 `jobs` 表第 3 列就是 `organization_id TEXT NOT NULL`，且
`UNIQUE (organization_id, consumed_plan_id)`、`idx_jobs_list/organization_id…` 索引都在，
`SCHEMA_VERSION = 4`（v4 只加了 `claim_client_instance_id`）。因此本轨道**没有新增迁移**，
只把「组织维度」贯通到 DTO 与过滤路径。这条是「落点需自行核实」的直接结果。

## 新增测试

| 落点 | 用例 | 断言要点 |
| --- | --- | --- |
| `packages/platform-api/src/dto/job.rs` | `the_persisted_job_vocabulary_carries_the_organization_dimension` | `JobDefinition` 序列化带 `organizationId`；省略该键的 `JobFilter` **无法反序列化** |
| `packages/platform-api/src/dto/job.rs` | `the_job_filter_cannot_be_declared_without_a_required_organization` | 声明形状断言：字段必填、非 `Option` |
| `packages/platform-api/src/ports/job.rs` | `the_port_pins_the_organization_dimension_into_the_persisted_vocabulary` | 契约文本必须声明组织语义 |
| `packages/runtime/src/job/repository/org_scope_tests.rs`（新文件，8 条） | 盖章 / 列举 / 读取 / 恢复 / 幂等作用域 / claim / 越组织写 / 越组织取消 | 跨组织一律「不存在 / 空 / StaleClaim」，不返回「无权限」 |
| `src-tauri/src/store/app_db_job_org_tests.rs`（新文件，4 条） | 盖章落库行 / 列举 / 读取 / 恢复扫描 | 同上，主体相同只换组织，排除「因主体不同而不可见」的假阳性 |

## 门禁（权威值：隔离 target 目录 `/tmp/datazen-job-org-scope-target`）

> 共享 target 目录会跨工作树串产物（协调者已确认）：cargo 对 path 依赖的指纹与产物文件名
> 不区分工作树路径，后跑的轨道会 fresh 复用**别的**工作树的 rlib。因此本轨道的**权威结论**
> 全部来自隔离目录；共享目录下的历史结果不作结论。

门禁首：`HEAD=8bf6eb5534ef8f32eca7ef57f7f1430499732d8c`，
工作区 sha256(`git status --porcelain=v1`)=`0433107f601564272cf39adba5512b7291aecf7cd0f2bb7f913bb82a18b7ca91`（36 项）。

```
$ cargo fmt --all -- --check
EXIT=0

$ cargo clippy -p datazen-platform-api -p datazen-runtime -p datazen --all-targets
EXIT=0        # 0 errors（warnings 均为既有风格告警：datazen (lib test) 213 warnings / 178 duplicates）

$ cargo check -p datazen-platform-api -p datazen-runtime -p datazen --all-targets
EXIT=0        # 0 errors

$ DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-platform-api --lib
running 224 tests
test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
EXIT=0

$ DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-runtime --lib
running 638 tests
test result: ok. 638 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
EXIT=0

$ DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib
running 1278 tests
test result: ok. 1269 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out; finished in 20.31s
EXIT=0
```

9 条 ignored 全部是既有条目（live PostgreSQL/MySQL 夹具、OS keychain、手工 golden 输出），
与本轨道无关。

### 附带跑的非门禁套件（同一隔离目录，退出码 0）

- `cargo test -p datazen-runtime --tests`：38 个集成测试目标全绿（最大一组 638 passed）。
- `cargo test -p datazen-feature-core --lib`：`367 passed; 0 failed`。
- `cargo test -p datazen-data-transfer --lib`：`226 passed; 0 failed`。
- `cargo test -p datazen-schema-diff --test job_handler`：`10 passed; 0 failed`。
- `cargo check --workspace --all-targets`：EXIT=0（全工作树可编译）。

## 遗留与待裁定项

1. **越界改动一处（已知会与 di-dyn 轨道产生文本冲突）**：`packages/feature-core/src/restore_job.rs`
   的两处 `JobDefinition` 测试夹具必须补 `organization_id`，否则该 crate 的 lib test 目标无法编译。
   任务书要求「不碰 `packages/feature-core/src/**`」，但 DTO 新增必填字段后**没有别的办法**让该 crate 编译。
   改动是 2 行、纯 `#[cfg(test)]` 代码；di-dyn 轨道当时未触碰该文件（实测两棵树该文件逐字相同）。
   若协调者希望由集成阶段统一处理，回退这两行即可，代价是 `cargo test -p datazen-feature-core` 编译失败。
2. **既有文档漂移，未修（本轨道范围外）**：`docs/architecture/platform/persistence-model.md` §4.1 写
   「桌面 AppDb 当前 `SCHEMA_VERSION` 为 3」，代码里是 `4`（v4 加 `claim_client_instance_id`）。
   与本轨道无关，未改。
3. **`InMemoryJobRepository::accept` 的新行为**：重复 jobId 现在返回 `BackendUnavailable("job identity is already admitted")`，
   与 SQLite 侧 `jobs.job_id` 主键冲突的口径一致（此前内存实现是静默覆盖）。现有测试全绿；
   若别处依赖「同 jobId 覆盖」语义，需要改用新的 jobId。
4. **`JobFilter` 不再实现 `Default`**：这是「组织维度必填」的直接后果（可选字段就等于允许跨组织列举）。
   代价是所有构造点必须显式写组织；`datazen-application` 的 `dto::job` 是同一类型的 re-export，无第二份定义。
5. **`JobRuntimeRepository::renew` 仍无 `ctx`**：端口签名里没有 `RequestContext`，因此续约不按组织过滤。
   它由 claim fencing 兜底（claim 只能在按组织核对过的行上产生），已在代码注释里写明。
6. **`RecoveryFilter` 未加组织字段**：`list_recoverable` 按 `ctx` 的组织过滤（`ctx` 是身份唯一来源，INV-01），
   与 `JobFilter` 的显式组织字段分工不同。若要「跨组织恢复扫描」，需要另加显式 opt-in 类型。

## 未开始

无（本轨道的范围内事项全部完成）。
