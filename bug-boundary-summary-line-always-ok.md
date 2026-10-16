# BUG · 边界检查在规则有 violation 时仍打印 ok

**状态**：开放 · 低 · 由预置提交 `179c6c8b6` 引入边界脚本，**非 C3 轨引入**

## 症状

`scripts/check-platform-crate-boundaries.mjs` 逐规则汇总行无条件打印 `ok <rule-id>`，
即使该规则本轮有 violation。真正能判定的只有**末尾汇总行**与**退出码**。

## 证据

F-11 变异探针日志（注入改名依赖后）：

| 行 | 内容 |
| --- | --- |
| 6 | `VIOLATION F-11  datazen-platform-infra …` |
| 16 | `ok F-11: datazen-platform-infra` |
| 17 | `FAIL — … 1 violation(s), 0 error(s)` |

EXIT=1。**退出码与汇总判据均正确，因此不构成门禁失效。**

## 影响

只看逐规则那一行就下结论的自动化（或人）会把失败读成通过。

## 修法

逐规则汇总行在该规则有 violation 时改印 `fail`。改动点：`RULES` 遍历后拼装汇总字符串处。

## 关闭条件

变异探针日志中不再出现「`VIOLATION F-11` 与同一行前缀 `ok F-11` 并存」。

---

来源：Wave-2 C3 独立验证（缺陷 D4）。