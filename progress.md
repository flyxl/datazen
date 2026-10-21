# 轨道 A：job-org-scope 进度台账

> **本文件在合并前必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）：
> `git rm progress.md && git commit -m "chore: destroy the progress ledger"`。
> 它在本轮被按协调者要求重建过一次（为了记下下面那条可迁移的教训链），重建不等于留存。

## 目标

让 job 的**持久化词汇**带上 `organization_id`，使多租户（web 形态）能按组织隔离 job 的
受理、列举与恢复。桌面是单租户，桌面行为必须完全不变。

## 已完成

| # | 事项 | 落点 | 状态 |
| --- | --- | --- | --- |
| 1 | `JobDefinition` / `JobFilter` 新增**必填** `organization_id`（`JobFilter` 相应移除 `Default` 派生） | `packages/platform-api/src/dto/job.rs` | 完成 |
| 2 | 端口契约文档补「组织维度」段：读路径按「不存在」、写路径按 claim fencing 拒绝 | `packages/platform-api/src/ports/job.rs` | 完成 |
| 3 | `InMemoryJobRepository`：受理盖章、幂等回执按 `(org, principal, key)` 作用域、已消费 planId 按 `(org, planId)` 唯一、列举/查询/恢复/认领/写路径按组织过滤 | `packages/runtime/src/job/repository.rs` | 完成 |
| 4 | `owner_claims.rs` / `runtime_methods.rs` 带上组织维度 | `packages/runtime/src/job/repository/` | 完成 |
| 5 | SQLite 侧：`read_record` 读出组织、`admit_job` 盖章、`list` 校验过滤器组织 | `src-tauri/src/store/app_db/jobs/` | 完成 |
| 6 | `renew` 的三处注释改写为**事实陈述**（续约路径不做组织比对；`JobClaim` 不带组织维度） | 端口契约 + 两个实现 | 完成 |
| 7 | 全仓 `JobDefinition` / `JobFilter` 构造点补字段 | 见 `git log --stat` | 完成 |
| 8 | 文档：端口 §4.3「Job 的组织维度（已实现）」；P9 协调文档补「这条谓词描述 P9 服务端形态」限定语 | `docs/architecture/platform/` | 完成 |
| 9 | 组织隔离单测 15 条 + 守卫自检 9 条 + SQLite 侧 3 条 | 见下 | 完成 |

### SQLite schema 迁移：无需新增

任务书 (e) 假设 `jobs` 表还没有 `organization_id`。**实测：已经有**——`src-tauri/src/store/app_db.rs`
的 `JOB_SCHEMA_V2` 里 `jobs` 表第 3 列就是 `organization_id TEXT NOT NULL`，`SCHEMA_VERSION = 4`
（v4 只加了 `claim_client_instance_id`）。因此本轨道**没有新增迁移**。

## 一条给后续所有轨的教训链（可迁移）

1. **默认门禁不编译 feature 分支** → 阻塞缺陷一路滑过。第 1 轮：`#[cfg(feature = "webdriver")]`
   里的 `JobFilter` 字面量没跟上字段变更，fmt/clippy/check/三套件全绿，而
   `pnpm tauri:build:webdriver` 编译失败。→ 协调者已把
   `cargo check -p datazen --features webdriver --all-targets` 加进所有门禁集。
2. **注释描述了代码不保证的不变量** → 第 2 轮：三处 `renew` 都写着「越组织续约会被拒」，
   而 `JobClaim` 不带组织维度，那条路径上「跨组织续约」根本不可表达（实测：对得上
   worker+generation 的续约在任何组织都成立）。→ 注释必须写成**事实陈述**，不写推断。
3. **修守卫精度时引入了漏报** → 第 2 轮：把「desync 区段仍可见」变成了「desync 区段被掩掉」，
   守卫从「能抓住」变成「抓不住」，而**没有任何编译错误、没有任何既有用例变红**。
   → 改守卫就是在改它的**检测能力本身**，每次都要用变异证明没有把「能抓住」换成「抓不住」。
4. **仓库里已有一份正确实现** → 第 3 轮：裸串词法在
   `packages/runtime/src/tunnel/source_scan.rs:89-110` 已经是对的（0 个 `#` 也认、掩码保行数）。
   → 遇到重复的词法/扫描逻辑，先找仓库里是否已有正确版本，别重写。

## 自验门禁（隔离 target `/tmp/datazen-job-org-scope-target`）

```
cargo fmt --all -- --check                                        EXIT=0
cargo clippy -p datazen-platform-api -p datazen-runtime -p datazen --all-targets
                                                              EXIT=0，0 errors
cargo check  -p datazen-platform-api -p datazen-runtime -p datazen --all-targets
                                                              EXIT=0，0 errors
cargo check  -p datazen --features webdriver --all-targets    EXIT=0，0 errors
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-platform-api --lib
   test result: ok. 224 passed; 0 failed; 0 ignored           EXIT=0
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-runtime --lib
   test result: ok. 642 passed; 0 failed; 0 ignored           EXIT=0
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib
   test result: ok. 1280 passed; 0 failed; 9 ignored          EXIT=0
```

（末轮数字见协调者收到的报告；9 条 ignored 全是既有 live 夹具条目。）

## 遗留与待裁定项

1. `packages/feature-core/src/restore_job.rs` 的 2 行 `JobDefinition` 夹具仍在：`JobDefinition`
   新增必填字段后，该 crate 的 lib test 目标无法编译，不改就编译不过。改动是 2 行、纯
   `#[cfg(test)]` 代码；回退这两行的代价是 `cargo test -p datazen-feature-core` 编译失败。
2. D-07（内存 `check_claim` 只比组织、SQLite 比组织+主体）、`persistence-model.md` 的
   `SCHEMA_VERSION` 3/4 漂移、claim 写路径的「存在性预言机」——均按协调者裁决留给 Wave 4。
