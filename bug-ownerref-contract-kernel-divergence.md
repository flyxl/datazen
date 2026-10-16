# B12：契约与内核各有一份同名 `OwnerRef`，3 个变体字段集不同，且差异承重

- **状态**：开放 · 已确认 · **需设计裁定**（不是迁移轨或协调者能单方面决定的）
- **归属**：既有设计债，**非本次下沉引入**；三处代码注释已登记。本轮只是把它从 B6 的误判里分离出来。
- **来源**：由 B6 改判而来。B6 的原判据「宿主重复定义契约」经实测**不成立** ——
  `src-tauri` 从无 `enum OwnerRef`，宿主 49 处引用全部已是 `datazen_platform_api::context::OwnerRef`。

## 实测

`grep -rn 'enum OwnerRef' src-tauri/ packages/` 只有两处：

| 位置 | 角色 |
| --- | --- |
| `packages/platform-api/src/context.rs:65` | 契约。与目标设计 [连接 §4](connection-management.md#4-dto-与字段定义) 的 `OwnerRef` **逐字段一致** |
| `packages/runtime/src/connection/types.rs:381` | **内核**。偏离目标形态 |

变体**名**相同（4 个、同序），但 4 个里 3 个字段集不同：

| 变体 | 契约（platform-api） | 内核（runtime） |
| --- | --- | --- |
| `Editor` | `client_instance_id`, `editor_session_id` | 同前 **+ `organization_id` + `principal_id` + `connection_id`** |
| `Job` | `job_id`, `stage_id: id::StageId` | **`organization_id`**, `job_id`, `stage_id: String` |
| `WorkflowBlock` | **`job_id`**: `JobId`, `block_id` | **`run_id`**: `RunId`, `block_id` |
| `ClientSession` | `client_instance_id`, **`purpose: String`** | 只有 `client_instance_id` |

serde 也不兼容：契约用 `tag="kind" + rename_all_fields="camelCase"`（字段名转 camelCase），
内核只有 `tag="kind" + rename_all="camelCase"`（**字段名仍是 snake_case**）。

## 为什么不能直接合并（差异是承重的）

- `packages/runtime/src/gateway/owner_binding.rs:170-183` —— 归属闸门直接解构 `organization_id` / `principal_id`
  做 CM-06 跨租户 / 跨主体拒绝。改用契约类型 ⇒ 闸门**结构上再也看不到组织与主体**。
- `packages/runtime/src/connection/types.rs:403+` —— `OwnerRef::hash()` 用 `organization_id` / `principal_id` /
  `connection_id` 生成 `runtimeEpoch` 指纹；`lease.rs:253` 按它批量处置资源。
- 反向（把内核字段补进契约）同样不行：`packages/platform-api/src/context.rs:3-5`、[连接 §4](connection-management.md#4-dto-与字段定义)
  与 `shared-boundaries-and-ports.md` 的 identity 规则都要求**身份只能由 adapter 构造、请求体字段不得覆盖身份**；
  给 `Editor` / `Job` 补身份字段等于把身份放进客户端可提交的契约类型，
  并会改动 jobs 落盘 JSON（`store/app_db/jobs/codec.rs` 解的是契约 `OwnerRef`）与 TS 契约。

## 已登记位置（三处，都在代码里）

- `packages/runtime/src/connection/types.rs:73-83` —— 「以下类型与 `platform-api` 存在同名物，但形状或语义确实不同，
  **因此刻意不去重**。合并它们会改动序列化后的线上字面量，属于行为变更，不在类型去重范围内。」`OwnerRef` 在表内。
- `packages/runtime/src/gateway/owner_binding.rs:100-122` —— 登记同一笔债，写明「本轨**不统一**这两个类型」、
  「`OwnerRef` 本身在本轨是**冻结**的：给 `OwnerRef::Job` 补 `principal_id` 会同时改变契约层语义，
  必须由有权限的人显式决定，不能由网关单方面加字段」。
- `packages/application/src/identity_policy.rs:120-136` —— 同样登记。

守卫测试 `packages/runtime/tests/owner_binding.rs:230-262`（`cm06_the_job_owner_variant_field_list_is_pinned`）
从 `types.rs` 原文抠出 `OwnerRef::Job` 的字段表并钉住；失败信息明确要求：若新增 `principal_id`，
H 层已具备拒绝条件，须补对偶断言并更新文件头台账。**它是故意的绊线，不是待清理的噪音。**

## 处置选项（待设计裁定，本轮未择）

- **A 重命名内核那份**（如 `KernelOwnerRef`）：唯一能让「全仓只剩一个 `enum OwnerRef`」字面成立的做法，零行为变更。
  实测代价：`packages/runtime` 下 43 个文件用内核类型（其中 27 个是测试 / 测试支撑），
  另有 `src-tauri/src/platform/session_backend/opening.rs:37` 与 `:325`（`rc::OwnerRef::Editor`），合计约 44 个文件；
  **必须逐文件区分** —— 同目录另有 14 个文件用的是契约类型，误替换会改动契约使用点。
  代价大，而收益只是让代理指标成立。
- **B 真做分层**：内核改存「契约 `OwnerRef` + 身份三字段」，闸门改从该字段取值。
  属跨 crate 契约 + CM-06 安全语义变更，需概要 / CM owner 裁定。
- **C 只登记不改**（当前状态）：债已三处登记，本文件是第四处，
  也是唯一在 `docs/` 侧的落点 —— 已在 `docs/architecture/platform/shared-boundaries-and-ports.md` 的类型来源声明处写明。

## 关闭条件

既不是「grep 只剩一处」（A 能满足但不动真问题），也不是「两边字段一致」（只有 B 能做到）。
真条件是：**两层的形状差异要么被消除，要么被明确裁定为长期允许，并把该裁定写进平台架构文档。**
