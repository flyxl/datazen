# Safe Mode 对齐 Drop

> 分支：`feat/safemode-drop`

| 层 | 行为 |
|----|------|
| 后端 `sql_guard` | Safe Mode 拦截 `DROP`（与 `TRUNCATE` 同级） |
| Schema 树 | 隐藏 Drop / Drop View |
| 索引页 | 隐藏删除索引按钮 |
| 设置文案 | Hint 含 TRUNCATE/DROP |
| E2E 夹具 | `executeSQL` / `withSafeModeOff` 临时关闭；`client-parity` 断言 DROP 被拦 |
