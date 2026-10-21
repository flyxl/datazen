# 进度台账：轨道 E — schema-diff reviewed 计划存储端口化

> 交付即销毁：本文件在轨道验收合并时必须删除。

## 目标

`packages/schema-diff/src/reviewed.rs` 的进程级全局 `static PLANS` 改成**端口 + 宿主注入的实例**；
规则留在包里，存储由端口抽象，访问按 `OrganizationScope` 隔离。

## 已完成

| 项 | 状态 |
|---|---|
| 包内定义 `ReviewedPlanStore` 端口（`put` / `get` / `remove`，方法一律带 `scope: &OrganizationScope`） | ✅ |
| 包内 `static PLANS` / `plans()` 删除，包内不再有进程级全局可变状态 | ✅ |
| `freeze*` / `consume` 改为 `(store, scope, …)` 签名，规则函数（`identity` / `validate_*` / `same_endpoint` / `physical_database_scope`）签名与行为不变 | ✅ |
| 宿主实现端口：`src-tauri/src/commands/schema_diff/reviewed_store.rs`（桌面单租户、进程内存） | ✅ |
| 宿主组装期持有实例并注入：`AppState.schema_diff_reviewed_plans` | ✅ |
| 宿主调用点按新签名传 `OrganizationScope::from_context(&ctx)` | ✅ |
| 跨组织访问读不到（宿主 5 条 + 包 3 条用例，逐条断言 `None` / `false`） | ✅ |
| 文档：`docs/architecture/backend/schema-diff.md` 新增「评审计划：规则留包，存储走端口」 | ✅ |

### 设计要点

- **行类型留在包里**：端口的行类型就是 `ReviewedPlan`，不另造结构。
- **越权一律按「不存在」**：`get` → `None`、`remove` → `false`、`put` 不落数据；**不**返回「无权限」（CM-05）。
- **有效期与容量上限由包给常量**（`REVIEWED_PLAN_TTL` = 30 min、`REVIEWED_PLAN_MAX_ENTRIES` = 128），
  存储实现据此裁剪，规则层与存储层读同一份时刻（`created_at_ms`，Unix 毫秒，可持久化）。
- **一次性消费语义保持**：`consume` 先 `get` → 校验 → 再 `remove`；校验失败**不**烧掉计划，
  并发部署时只有一个 `remove` 成功（与改造前一致）。
- **桌面持久化选择：进程内存**。桌面是单租户，评审计划本来就活在进程内，行为与改造前完全一致。

## 门禁实测

工作目录：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-schema-diff-reviewed-port`
`CARGO_TARGET_DIR=…/.worktrees/datazen-schema-diff-reviewed-port/target`

| 命令 | 退出码 | 结论行 |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | 无 diff |
| `cargo clippy -p datazen-schema-diff -p datazen --all-targets` | 0 | 427 warnings（改造前同口径 428，未新增） |
| `cargo check -p datazen-schema-diff -p datazen --all-targets` | 0 | — |
| `cargo check -p datazen --features webdriver --all-targets` | 0 | — |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-schema-diff --lib` | 0 | `151 passed; 0 failed`（基线 148，+3 新隔离用例） |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | `1271 passed; 0 failed; 9 ignored`（本轨新增 5 条 ⇒ 改前 1266） |

已知 flake：`platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped`
高负载下 `TimedOut`，本轮未命中。

## 验收自证

1. `grep -rn "static PLANS" packages/schema-diff/src/` → **0 命中**；
   `grep -rnE "^static [A-Z_]+ *:" packages/schema-diff/src/` → **0 命中**。
2. 规则函数仍在 `packages/schema-diff/src/reviewed.rs`，签名未变：
   `identity` / `validate_source_connection_snapshot` / `same_endpoint` /
   `physical_database_scope` / `validate_snapshot` / `validate_object_snapshot`。
3. 跨组织用例（逐条断言，非模糊断言）：
   - 包：`reviewed_plan_store_hides_plans_from_other_organizations`（`get`→`is_none()`、
     `remove`→`false`、本组织 `get`→`is_some()`）、
     `reviewed_plan_consume_rejects_foreign_organization_scope`（错误文案逐字等于
     `"Plan expired or already executed; compare again"`，且本组织随后仍可消费）、
     `reviewed_plan_store_drops_writes_from_other_organizations`。
   - 宿主：`desktop_store_hides_plans_from_other_organizations`、
     `desktop_store_refuses_to_remove_other_organizations_plans`、
     `desktop_store_drops_writes_from_other_organizations`、
     `desktop_store_without_identity_denies_every_organization`、
     `desktop_store_consumes_a_plan_exactly_once`。
4. 桌面行为不变：既有 host / schema-diff 单测全绿，**未删任何 host 用例**（diff 中移除的
   3 个 `#[tokio::test]` 全部是包内被搬到 `reviewed_store_tests.rs` 的同 3 个用例，断言未改）。
5. 门禁全绿（见上表）。

## 遗留与待裁定

- **包行数**：生产码 606 → 708（+102：端口契约 + 文档注释），测试码 816 → 1062
  （+246：搬迁的 3 个用例 + 3 条新隔离用例 + 测试替身存储）。包**没有**被掏空：
  规则、类型、目录校验全部留在包里。
- **A 轨重叠**：`packages/schema-diff/tests/job_handler.rs` 与
  `src-tauri/src/commands/schema_diff/job/desktop.rs` 本轨**未触碰**，合流无冲突面。
- **同族未改造**：`packages/data-sync/src/plan_store/`、`packages/data-transfer/src/plan_store/`
  仍有各自的进程内计划存储，不在本轨范围。
- **待裁定**：是否把桌面评审计划落到 `{appData}/datazen.sqlite`（当前选进程内存；
  落盘会让「一次性计划」变成跨会话残留，需额外处理过期与并发消费）。
