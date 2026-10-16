# B15：门禁的逐规则汇总行在规则产出 error 时仍打印 ok（与 B3 同类，走 error 通道）

- **状态**：开放 · 已确认（代码路径核实；未做端到端复现）
- **归属**：与 B3 同源、同一函数族，**未随 B3 的修复一并解决** —— B3 的修复范围被明确限定为「按 violation 数打印」，本条的触发条件是 `errors`。
- **发现**：Wave-4 · w4-b3-summary 的独立验证方（2f07527c）在复核 B3 时顺带指出；协调者已独立核实代码路径。

## 实测（代码路径）

`scripts/check-platform-crate-boundaries.mjs`：

- `ruleVerdict(ruleId, violations)` 定义在 `:701`，入参**只有** `violations`；
- 逐规则汇总行在 `:752` 调用 `ruleVerdict(rule.rule, result.violations)`；
- 同文件 `:382`（`checkTsLayer` 的 catch）会往 `errors` 推 ``${rule.id}: could not scan ${layer.path}: ${cause.message}``；
- 而 `:528` 的 `evaluated.push({ rule: rule.id, ... })` 在 `files > 0` 时**仍会执行**。

⇒ 当某条 TS 层规则扫到了文件、但扫描中途失败时，同一次运行里会同时出现
「该规则进了 `evaluated` ⇒ 汇总行打印 `ok F-07:`」与「该规则进了 `errors` ⇒ 另有 `ERROR F-07:` 行」。

## 危害

与 B3 完全同类：一次运行里对**同一条规则**给出两个互相矛盾的判决。差别只在通道 —— B3 修的是 violation 通道，
这条走 error 通道。**现有任何变异探针都不触发它**（`scripts/platform-arch-selfcheck.mjs` 的 8 个变异全部走 violation 通道），
所以它与 B3 一样可以长期存活。

## 修法（待择，属门禁语义决定）

1. 让汇总行同时考虑 `errors`（把 error 也归因到规则 id，打印 `fail` 或单列 `error`）；
2. 或让 `:528` 在本次扫描已推 error 时**不**把该规则计入 `evaluated`；
3. 或把「扫到文件但扫描失败」的规则从汇总行里排除、只出现在 ERROR 行 —— 但那样必须保证不会因此**静默漏检**。

选哪种要先裁定「一次扫描失败时，该规则在汇总行里应当是什么」，**不在 B3 的修复范围内**，也不由 w4-b3-summary 承担。

## 关闭条件

构造一次「TS 层规则扫到文件但扫描中途失败」的运行，逐规则汇总行与该规则的 ERROR 行**不再给出互相矛盾的判决**，
且门禁正常路径结论行仍为 `0 violation(s), 0 error(s)`。
