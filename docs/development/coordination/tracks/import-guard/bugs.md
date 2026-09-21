# Track: import-guard — Bug 台账

## import-guard-BUG-008 · Blocker · 已修复

**标题**：护栏在本地全量检出红 8 条（作用域误把 gitignored 外部仓库树算作阻断范围）

**发现者/时机**：协调者合流门禁（`git merge --no-ff feature/import-guard` → `f2514033b` 后在主检出跑
`node scripts/check-driver-import-boundaries.mjs`）。worktree 内 Coder/Tester 双绿是**环境假绿**。

**重现**（主检出，基准 `f2514033b`，本地已 clone git driver + 已 stage Pro EP）：

```
node scripts/check-driver-import-boundaries.mjs   # EXIT=1，FAILED: 8 violation(s)
```

**实测违规清单（全部落在 gitignored 的外部树，`git check-ignore` 已核实）**：

| 规则 | 文件:行 | 说明 |
| --- | --- | --- |
| R1 | `packages/drivers/superset/ui/SupersetConnectionFields.tsx:3` | `import { useI18n } from '../../../../src/hooks/useI18n'` |
| R1 | `packages/drivers/superset/ui/SupersetSchemaTree.tsx:19` | 同上 |
| R2 | `packages/pro-extensions/sql-editor-pro/src/intentions/__tests__/intentionCodeActions.test.ts:119,141` | EP 自身单测切语言 |
| R2 | `packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts:28,32,36,39` | 同上（`en`/`zh-CN`/`fr`） |

`.gitignore:79 /packages/drivers/*`、`.gitignore:68 packages/pro-extensions/` 覆盖上述路径；
`drivers-registry.json` 里 `superset`/`olap`/`kiwi` 是 `source: git` 的外部驱动仓库，
`packages/pro-extensions/*` 每个子包是独立 git 仓库（本仓不跟踪）。

**期望**：本仓不跟踪的外部树**不应让本地门禁变红**——否则开发者第一反应是关掉/绕过护栏，比没护栏更糟。
**实际**：R1/R2 一律 blocking，本地 `--drivers=all` + Pro 检出必红 8 条。

**协调者裁定（修复方向，按此实现）**：

1. **作用域按「是否被本仓跟踪」判定，而不是按目录通配豁免**：命中违规后，用 `git check-ignore`
   （或等价：`git ls-files -o -i --exclude-standard` 预计算集合）判定该文件是否属于 gitignored 外部树；
   属于 → 降级为 **advisory**（仍逐条点名输出，计入 advisory 计数，不影响 exit code）；
   不属于 → 保持 blocking。
   - **实现约束**：分类只在**发现违规时**做（最多 8 次 git 调用），禁止在 `walk()` 热路径上对 1489 个文件逐个调 git。
   - 输出必须让开发者一眼看懂：advisory 段补一句「external (untracked) repo — contract drift to be fixed in that repo, not here」。
2. **R3 保持 advisory 不变**（现状 4 处，已裁定）；`RULES.R3.blocking` 仍为 false。
3. **allowlist 仍恰好 2 条**，`R2_FILE_CARVEOUTS` 仍恰好 2 个精确文件；**不得**为 superset/editor-pro 新增豁免条目
   （它们由第 1 条的作用域规则吸收，不占豁免额度）。
4. **单测补齐**：新增分支覆盖「ignored 外部树违规 → advisory 且 exit 0」「tracked 文件违规 → blocking 且 exit 1」，
   注入式自证两类各一次（结束还原、`git status` 干净）。
5. **文档同步**：契约 2.1.2 / 2.4.2（及 2.6 表格）写明「阻断范围 = 本仓跟踪的源码；
   gitignored 外部仓库树（git driver clone、`packages/pro-extensions/*`）以 advisory 报告，
   漂移由该仓库自行整改」，并**如实记录当前外部漂移**：superset 2 处 R1、editor-pro 6 处 R2（含文件:行）。
   顺带关掉上一轮 Tester 留待的 2 条 Nit：① 2.6 单测格对 branch 95.42% 未触发的 6 处 `??` 兜底描述不完整；
   ② `scripts/ci-local.sh` 步骤编号 `3.4/11` 排在 `3.3/11` 之前。
6. **另立移交项（不属本轨修复）**：superset 外部驱动仓库的 2 处 R1 需在其自身仓库整改
   （换源 `@datazen/ui`）；editor-pro 的 6 处 R2 是其单测合法用法，若日后要求收敛再议。
   本轨只在文档里登记，不动那两个外部仓。

**验收增补（在原 8 条之上）**：

- 主检出（`--drivers=all` + Pro 已 stage）跑 `node scripts/check-driver-import-boundaries.mjs` → **exit 0**，
  且输出里 8 条以 advisory 形态点名（R1×2 + R2×6 + R3×4 = 12 advisory）。
- 在**本仓跟踪**的驱动文件里注入 R1 → 仍 blocking 且 exit 1（证明降级只作用于外部树，没把口子开进本仓）。
- `npx vitest run scripts` 全绿；覆盖数字如实更新；`npx tsc --noEmit` 0；`npx vite build` exit 0。

**状态**：待修复 → **待复测**（接管代理补齐，修复 commit `17c3d5cc7` 代码+单测 / `485d75797` 文档回扫+Nit；主检出 `--root` 复跑 exit 0 + 12 advisory，本仓跟踪文件注入 R1 反证 exit 1，证据见本轨 `progress.md`「BUG-008 接管实施记录」）→ **已修复**（第 2 轮全新 Tester 实例 66 次工具调用、10 项核查全绿；该实例在写记录时遭网络中断，正文由协调者从其 transcript 恢复，见 `progress.md`「BUG-008 第 2 轮复测记录」+ 协调者在合并后主检出的三条交叉核对。二次合流 `b22b41ac8`）
