# B13：边界门禁自身的规则范围描述过时（文件头写 F-01..F-08，实际 11 条；两条错误信息写 F-01..F-07）

- **状态**：开放 · 已确认（静态检索；未做行为验证，因为它是纯描述）
- **归属**：既有文档债（注释 + 错误信息），**非本波引入**。由 Wave-4 · w4-b3-summary 轨在修 B3 时顺带发现。
- **不阻断任何轨**：只影响可读性与排障时的自我描述。

## 实测

`scripts/check-platform-crate-boundaries.mjs` 的 `RULES` 实测 **11 条**
（`grep -cE '^\s*id: .F-[0-9]+'` = 11，即 F-01…F-11）。但同文件三处自我描述停留在旧范围：

| 位置 | 现有文字 | 问题 |
| --- | --- | --- |
| `:3` | `Platform crate dependency boundary gate — F-01..F-08 of` | 实际到 F-11 |
| `:299` | `the guard can no longer claim to implement F-01..F-07.` | 实际到 F-11 |
| `:472` | `the guard cannot verify it implements F-01..F-07` | 实际到 F-11 |

另有一处在**仓库工作流**里（同一次盘点发现）：

| 位置 | 现有文字 | 问题 |
| --- | --- | --- |
| `.github/workflows/ci.yml:307` | `name: Platform crate dependency boundaries (F-01..F-07)` | 实际到 F-11；该步跑的是同一个门禁 |

CI 的步名会显示在每次运行的界面上，误导面比脚本注释更大。

后两处是**错误信息**，出问题时打印给排障者，误导性更强。

## 影响

- 排障时按 F-01..F-07 去 spec 表里找，会漏掉 F-08…F-11 四条规则。
- 门禁的 spec 一致性检查（`checkSpecConsistency`）本身按代码里的 `RULES` 遍历，**不受这些字符串影响**，
  所以这是纯描述债，不会造成漏检 —— **这正是它长期存活的原因**。

## 修法

把三处范围字样改成不写死数字的表述（写成正确范围，或改述为「本表列出的全部规则」），
并在 spec 表增删规则时避免再次写死。**改完必须复跑门禁**确认结论行仍为 `0 violation(s), 0 error(s)`。

## 关闭条件

`scripts/check-platform-crate-boundaries.mjs` 中不再出现写死为 `F-01..F-0N`（N 与 `RULES` 条数不符）的范围字样，
且门禁正常路径退出码 0、结论行的 violation / error 仍为 0/0。
