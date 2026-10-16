# B17：同一列混用两种时刻形状 —— 秒级 `…T00:00:00Z` 与毫秒级 `…T00:00:00.000Z` 的字典序在整秒处翻转

- **状态**：开放 · 已确认（形状与比较点已实测；未做端到端排序复现）
- **归属**：由 `Clock` 端口落地（Wave-4 · w4-clock）**暴露**的既有债，**非该轨引入** —— 秒级写入点在本波之前就存在。
  该轨已把这条写进 `docs/architecture/platform/shared-boundaries-and-ports.md` §4.9 的「已知遗留」。

## 实测

`Timestamp` 的比较是**字符串字典序**。两种形状混在一列时，字典序在整秒处翻转：

- 毫秒级： `2026-01-01T00:00:00.000Z`
- 秒级：　 `2026-01-01T00:00:00Z`

逐位比较到 `00:00:00` 后，毫秒形状的下一位是 `.`（U+002E），秒级形状的下一位是 `Z`（U+005A）。
`.` < `Z` ⇒ **同一时刻的秒级写法会被判为「大于」毫秒级写法**。

## 已实测的混用点

| 位置 | 形状 | 说明 |
| --- | --- | --- |
| `packages/platform-api/src/ports/clock.rs` 的 `SystemClock::now()` | 毫秒 `to_rfc3339_opts(SecondsFormat::Millis, true)` | 本波新增；全仓唯一读系统时间的生产实现 |
| `packages/runtime/src/job/time.rs` 的 `after_seconds()`（约 :85-92） | 秒级 `%Y-%m-%dT%H:%M:%SZ` | 只做算术、不读系统时间；**未改** |
| 宿主写 `expires_at`：`src-tauri/src/commands/schema_diff/job.rs:258` / `:681` | 秒级 | 与 `packages/schema-diff/src/job/plan.rs:193` / `:223` 的 `expires_at < now` 比较 |

`after_seconds` 的调用点：`packages/runtime/src/job/repository.rs:629` / `:667`、
`src-tauri/src/store/app_db/jobs/runtime_ops.rs:269` / `:305` / `:489`。

## 本波引入的一处**行为变更**（提交方已披露，需本条一并跟踪）

提交边界 `committed_at` 由秒级变为端口统一的**毫秒级**。唯一受影响的比较是 `plan.rs` 的 `expires_at < now`，
而 `expires_at` 仍由宿主写秒级 ⇒ 差异只在**同一秒**边界：整秒到期即判过期（更严）。
**该边界目前无测试覆盖**（提交方未声称有；w4-clock 的独立验证方被要求核实这一点）。

## 危害

- 任何把 `Clock` 输出与上述秒级输出放进**同一列 / 同一排序**的查询或断言，都会在整秒边界得到错误次序。
- 现网未爆，是因为两者目前落在不同的判定里。一旦某处变成一侧毫秒、一侧秒级，就会出现「同一秒内判反」
  而**没有任何测试会红**。

## 修法（待择，属独立变更）

1. **收口到单一形状**（首选）：把 `after_seconds` 与宿主那两个 `expires_at` 写入点也改用 `Clock` / 毫秒定宽形状，
   彻底消除混用面。`Clock` 的文档口径本来就是「只生产、不解析」。
2. 或让 `Timestamp` 的比较**不依赖字符串序**（解析后比较），代价是比较点全部要能容忍非法输入。
3. 或在 `Timestamp::new` 上加形状断言（拒绝秒级），把混用变成构造期的显式失败。

`shared-boundaries-and-ports.md` §4.9 已记「收口前不要把它与 `Clock` 的输出放进同一排序」。

## 关闭条件

全仓不存在两种时刻形状可进入同一排序的路径（形状收口），**或** `Timestamp` 的比较不再依赖字符串序；
且必须给出「同一整秒的两种写法排序正确」的实测断言。
