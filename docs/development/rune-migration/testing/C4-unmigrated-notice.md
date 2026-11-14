# TN C4 未迁移区域提示 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`061ba45 feat(rune): the unmigrated areas say so and keep the React pages (issue-rune C4)`
- 改动面：`crates/datazen-ui/src/lib.rs`（`Section` 增一变体 `Wapps`、`ALL` 6→7、`key`/`label` 各增一臂；`page_body` 增一臂；新增 `wapps_page`；新增 1 个运行时测试）。只新增，不动现有组件的行为（plan 的拆分规则）。

## 实现

- 未迁移区域 = 产品有、Rune 无。判定依据：React 侧 `packages/frontend/src/windows/`（backup、connection、dashboard、data-sync、data-transfer、main、onboarding、schema-diff、settings、wapps、workflow、workspace 共 12 个窗口）对照 Rune 侧 6 个 Section 的已迁移面——**未迁移的是 WAPPS 管理**（React `WappManagementPage`/`InstallWappDialog`）与 **Onboarding 向导**（React onboarding 窗口）；schema-diff 即已迁移的 Compare，其余全部有 Rune 对应。
- 新增 `Section::Wapps`（导航标签 WAPPS，位于导航末位），页面为统一通告："Not yet migrated"，逐区域写明对应的 React 页面还在；WAPPS 条目同时写明 WAPP 的 iframe 不进 Rune（plan 不变量 #6）。
- 措辞只用 "still there"；整页（及全仓界面文案）不出现 "replaced"/"removed" 式声明——现有文案里的 "unchanged"/"preview" 本就是反声明。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_unmigrated_areas_say_not_yet_migrated_and_keep_the_react_pages` | datazen-ui（runtime，经真实侧边栏 `select(Section::Wapps)` + `Runtime::accessibility_json`） | nav-wapps role=tab、name=WAPPS；页面两条目挂载（wapps-pending、onboarding-pending）；页面文案含 "Not yet migrated"，分别含 "The React page is still there" 与 "The React wizard is still there"，并含 "do not go into Rune"；整页文案小写后不含 "replaced"、不含 "removed" |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 113 passed，新增 1 个测试；datazen-ui-api 54 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（既有 3 条警告，非本次引入） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 未迁移区域在 Rune 里有入口、有状态、有 React 指向；界面不出现已替换、已删除的声明。遗留：若日后 React 新增窗口或区域，需同步在本页登记或迁移（属后续任务，不在本任务判据内）。
