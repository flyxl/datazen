# T12 测试报告：文档更新与全量回归

- 被测对象：docs 提交 `f70709002`（`docs(issue-32): document value presentation in user guide, DataTable arch and extensibility (T12)`，3 文档文件 +185 行）+ noNativeSelect 修复两笔提交 fix `fa4182a2c`（`fix(issue-32): use ui Select instead of native select (T12 noNativeSelect)`，2 生产文件 +131/−135）与 test `fef1ae9fa`（`test(issue-32): adapt select tests to ui Select (T12)`，2 测试文件 +71/−21）。基线 `6b0a538aa`（close T11）。
- 分支：`feature/issue-32-value-presentation`（HEAD `fef1ae9fa`，验证开始时工作区干净，`git status` 无未提交差异；结束时除本报告外仍干净，见 §6）。
- 测试角色：独立 tester。未修改任何被测源码、测试或文档；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §12（文档落地）、§10-C（规则与双端恢复）、§5/§6/§7（与文档事实相关项）；`docs/archive/development/issue-32-value-presentation-prd.md` §6 验收标准 1–12；T01–T11 测试报告（验收残余缺口的对照基线）。
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，cargo（rustup stable，`~/.cargo/bin`），系统时区 UTC（探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `~/workspace/t12-probe/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志见 `~/workspace/t12-p-*.log`、`~/workspace/t12-p12.log`、`~/workspace/t12-p2.log`、`~/workspace/t12-p3.log`；另有 G1–G6 日志 `~/workspace/t12-g*.log` 与 `~/workspace/t12-full-vitest-rerun.log`、`~/workspace/t12-g2b-isolation.log`、`~/workspace/t12-g2c-isolation-default-timeout.log`、`~/workspace/t12-i18n-sync.log`、`~/workspace/t12-prettier.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T11 报告相同情况）；typecheck 与全量改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。G2 全量运行时 `PATH` 含 `~/.cargo/bin`（任务要求的守卫条件；本次 G1/G3/G4 亦在含 cargo 的 `PATH` 下跑，全程未出现 T12 coder 报告提到的 `spawnSync cargo ENOENT`）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/__tests__/noNativeSelect.test.ts --retry=0` | G1a：noNativeSelect 守卫 |
| `npx vitest run <Dialog / Entries / Section / valuePresentationRules 4 文件> --retry=0` | G1b：T12 定向（修复人申报 142/142 含守卫 1 项） |
| `npx -y pnpm@12.18.0 test`（日志 `~/workspace/t12-full-vitest-rerun.log`，单独记录退出码） | G2：全量 vitest 重跑 |
| `npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/components/DataTable packages/frontend/src/windows/settings --retry=0` | G3：严格模式抽查（coder 申报 98 文件 1182/1182） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G4：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G5：命名门禁 |
| `cargo test -p datazen --lib --features settings`（`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`）与 `cargo check -p datazen_desktop --lib --tests` 与 `cargo fmt --check` 与 `cargo test -p datazen_desktop --lib --no-run` | G6：Rust（coder 申报 settings 57/57；桌面 test 预期本地无法链接） |
| 三文档逐节核对 + 链接脚本 `node ~/workspace/t12-p4-links.mjs`（已随探针删除，方法见 §2.2 P4）+ `node scripts/i18n-sync-check.mjs` | G7：文档审读 |
| `git show f70709002 / fa4182a2c / fef1ae9fa` 全 diff + grep 新增行 `any` / 跳过断言 + Prettier + `git diff --check` | G8：改动范围审读 |
| 探针项目内 `./node_modules/.bin/vitest run --retry=0`（P1 5 项 + P2 3 项 + P3 4 项） | 对抗性探针 P1–P3；P4 为链接脚本，P5 为 shell 核查 |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | 守卫与定向 | noNativeSelect 1 项 + 定向 4 文件 | ✅ **142/142 通过**（守卫 1 + 定向 141：Dialog 25、Entries 2、Section 44、valuePresentationRules 70；退出码均为 0；与修复人申报逐项一致） |
| G2 | 全量 vitest 重跑 | `pnpm test` 全量，日志 `~/workspace/t12-full-vitest-rerun.log` | ⚠️ **9538 通过 / 1 失败 / 1 跳过**（855 文件：854 通过、1 失败；总计 9540；耗时 2127.13s，约 35 分钟，06:57:42–07:33:09 UTC；退出码非 0，pnpm 打印 `[ELIFECYCLE] Test failed`）。唯一失败是 `scripts/__tests__/check-driver-type-isolation.test.ts` 的 10s 超时（非断言失败），已证实为环境原因（独立重跑 36/36 两次通过），见 §7。noNativeSelect（coder 首轮的失败项）本次在全量中通过。1 跳过是 `cellEditorCandidates.test.ts` 的既有 1 项跳过，与 T12 无关 |
| G3 | 严格模式抽查 | valuePresentation + DataTable + windows/settings 三目录 | ✅ **98 文件 1182/1182 通过**（退出码 0；与 coder 申报逐项一致；耗时 520.69s） |
| G4 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 与 tsc 7.0.2，退出码均为 0） |
| G5 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2592 TS 文件、8 wdio spec 树；退出码 0。TS 文件数与 T11 回归验证的 2592 一致——T12 无新增 TS 文件，只有既有文件改动） |
| G6 | Rust | cargo test settings / cargo check desktop / fmt / desktop test 链接尝试 | ✅ settings **57/57**（与 coder 申报一致）；`cargo check -p datazen_desktop --lib --tests` 退出码 0（51 warnings）；`cargo fmt --check` 退出码 0；桌面 `cargo test --lib` 本地无法链接（`-lwebkit2gtk-4.1` / `-lgtk-3` / `-lgdk-3` / `-lgdk_pixbuf-2.0` 缺失），属环境限制留给 CI，见 §2.1 明细 |
| G7 | 文档审读 | 三文档逐节核对 + 链接逐条 + 常量/锚点/同步链路对照代码 + PRD 1–12 对照 T01–T11 | ✅ 通过（事实逐项对代码成立；4 条新增链接 + 全部既有相对链接可达，16/16；无「拟新增」写成已交付；无英文版遗漏；PRD 7/8/11 的残余缺口未被写成全通过，见 §3） |
| G8 | 改动范围与源码审读 | 三提交全 diff + grep + Prettier + diff-check | ✅ 通过（分拆、any/跳过、断言、守卫、行为等价、ui/Select 用法一致性均通过；见 §2.3） |

> G6 明细：
>
> | 命令 | 结果 |
> |------|------|
> | `cargo test -p datazen --lib --features settings`（`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`） | ✅ 57 passed，0 failed，退出码 0（日志 `~/workspace/t12-g6a.log`；与 coder 申报 57/57 一致） |
> | `cargo check -p datazen_desktop --lib --tests` | ✅ 退出码 0（51 warnings，均为既有警告；日志 `~/workspace/t12-g6b.log`） |
> | `cargo fmt --check` | ✅ 退出码 0（日志 `~/workspace/t12-g6-fmt.log`） |
> | `cargo test -p datazen_desktop --lib --no-run` | ⚠️ 链接失败（环境限制，非代码问题）：`error: linking with 'cc' failed`，`rust-lld: error: unable to find library -lgdk-3 / -lgdk_pixbuf-2.0 / -lwebkit2gtk-4.1 / -lgtk-3`（日志 `~/workspace/t12-g6c.log`）；未装系统依赖，留给 CI。另：`pkg-config --exists webkit2gtk-4.1` 与 `gtk+-3.0` 均缺失，见 §2.2 P5 |

### 2.2 对抗性探针（~/workspace/t12-probe，独立于 coder 自带测试编写）

探针方法总述：探针项目 `~/workspace/t12-probe/`（vitest 4.1.10，`node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，`vi.mock` 同样用绝对路径拦截 `stores/settingsStore` / `hooks/useI18n`（i18n mock 为 key 直通，T07 惯例），不污染仓库工作区；探针按任务要求用后删除，原始日志见 `~/workspace/t12-p-*.log`）。P1/P2 复用仓库测试的渲染脚手架（Dialog 的 `makeSnapshot` / `renderDialog` 与 Section 的 `renderSection`，含 T11 语义的 mock store），但用例与断言均为 tester 独立编写。

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | ui/Select 行为抽查 | P1a：Dialog 格式 iso-tz → locale → iso-tz，预览随之变（locale 文本含 2024、非 ISO 串）再变回锚点 `2024-01-15T19:00:00+08:00`，原值 `1705316400000` 不动；P1b：同一样本 `1705316400`，Unix 秒读数为 `2024-01-15T19:00:00+08:00`，切毫秒后变为 1970 年读数；P1c：重选已选值——打开格式下拉后 `getByText('settings.valuePresentation.format.iso-tz')` 抛多元素歧义，`select-option` 仅 3 项（只命中打开的 listbox），重选后预览不变；P1d：非法 IANA（`not a zone` 与 `Mars/Olympus`）经新 Select 切 iana 后保存仍被拒、就地报 `invalidConfig`、`onSave` 未调；P1e：RuleEditor 全字段规则保存（3 个 PatternField mode + interpretation/format/display-tz/source-tz 共 7 个渲染态 Select）后写回值与仓库既有 full-rule 期望逐字一致 | ✅ 5/5 通过 |
| P2 | 可访问性抽查 | P2a：Dialog 4 个 Select 触发器的 `aria-label` 分别等于传入的 title（interpretation / format / displayTimeZone / sourceTimeZone 四 key），`getByRole('button', {name})` 可定位同一触发器；旧 `label[for]` 已不存在；P2b：RuleEditor 3 个 PatternField mode Select 的 `aria-label` 等于字段 label（table / column / typeName 三 key）；P2c：其余 4 个 SettingRow Select 的 `aria-label` 为通用 placeholder（key 直通 mock 下读到 `select.placeholder`，真实 en 文案为 `Select…`），记录为观察项（见 O-2） | ✅ 3/3 通过 |
| P3 | 文档事实抽查 | P3a/P3b：双锚点经真实贡献 `prepare/present` 逐字重算（`2026-01-15T14:30:00Z` → `2026-01-15T22:30:00+08:00`；`1705316400000` → `2024-01-15T19:00:00+08:00`）；P3c：上限常量与文档数字一致（规则 20 / 模式 128 / 规则名 64 / 覆盖 200）；P3d：DST 切换日双义墙上时间 `2024-11-03 01:30` America/New_York 拒绝不猜测（`unchanged / dst-ambiguous`） | ✅ 4/4 通过 |
| P4 | 链接抽查 | node 脚本逐条解析三文档全部相对链接并校验目标文件存在，另校验 §14 / §13 / §1.7 章节标题存在 | ✅ 16/16 通过（12 条相对链接 + 4 处章节标题；含 4 条 T12 新增链接） |
| P5 | E2E 缺失核实 | `e2e/specs/` 全量清单 grep（value/presentation/timestamp/time-display 均无）；`pkg-config` 核查 webkit2gtk-4.1 与 gtk+-3.0；G6c 桌面链接失败作旁证 | ✅ 属实：无 valuePresentation 旅程 spec；沙箱内 Tauri webdriver 构建确实无法链接，E2E 未跑留给 CI（见 §7） |

> 探针总计：**12 项 vitest 探针 + 16 项链接检查全部通过**（P1 5/5、P2 3/3、P3 4/4、P4 16/16）。
>
> 探针过程说明：首轮 2 项失败均为**探针脚手架错误**，代码未动，修正后通过：(1) P3 首版 import 用模板字面量拼接绝对路径，esbuild 拒绝（import 必须是静态字符串），改为字面量绝对路径；(2) P2c 首版断言 `aria-label` 为 `Select…`，实际为 `select.placeholder`——前端 Select 包装层把 placeholder 换成翻译文案 `t('select.placeholder')`，探针的 i18n mock 为 key 直通故读到 key 本身（真实 en 文案为 `Select…`）；另 P2c 首版未注意到源时区下拉只在 text/native 解释下渲染（默认 unix-ms 下该 Select 不存在），补切 interpretation 到 text 后通过。P1c 的 `getByText` 歧义断言直接复核了修复人的说法（见 §2.3 T9）。

### 2.3 文档审读与改动范围明细（G7/G8）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 三提交改动范围 | `git show` 三提交 `--stat` + 全 diff 通读 | ✅ 基线 `6b0a538aa` 到 HEAD 的 diff 共 7 个不同文件：docs 3 文件 +185/−0（指南 §14 +80、data-table §14 与 §13 影响表 +80、extensibility §1.7 +25），fix 2 文件 +131/−135，test 2 文件 +71/−21。全部在 T12 范围内；tasks.md 未动；无无关改动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 三提交新增行 `: any` / `as any` / `<any>` / `any[]`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe`、另 grep `ts-ignore` / `ts-expect-error` / `eslint-disable` / `TODO` / `FIXME` | ✅ 新增行中 `any` 0 处；新测试 0 处跳过；无 ts 逃逸、无 TODO |
| T3 | docs/fix/test 提交分拆 | 三提交 `--name-only` 对照 | ✅ docs 提交仅 3 个文档文件；fix 提交仅 2 个生产文件（无测试文件混入）；test 提交仅 2 个测试文件（无生产改动） |
| T4 | test 提交无断言删除、无期望放宽 | `git show fef1ae9fa` 删除行审读 | ✅ 删除的 15 行全是驱动调用（Dialog 的 3 处 `fireEvent.change` + Section 的 12 处 `setField`），被删除行中 `expect(` 0 处；全部期望断言原样保留，新增的只是两个 helper（`pickSelectOption` / `setSelect`）与调用点改写。修复人称 test 提交 amend 时“仅并入 hook 留下的 prettier 换行”——核实：`-w` diff 与原 diff 行数一致（96 行），无纯空白改动；新增的 2 个空行属于 helper 插入自带的格式，无独立格式化改动可辨；实质主张（无生产改动、无无关改动）成立，见 O-4 |
| T5 | fix 提交的 9 处替换与 5 个守卫 | 全 diff 通读 + 基线对照 | ✅ Dialog 4 处（interpretation / format / display-timezone / source-timezone）与 RuleEditor 5 处（interpretation / format / display-timezone / source-timezone / PatternField mode）原生 select 全部改用 `components/ui/Select`；RuleEditor 新增 5 个守卫函数（`isPatternMode` / `isInterpretationChoice` / `isTimestampOutputFormat` / `isTimeZoneKindChoice` / `isSourceTimeZoneChoice`），基线中 0 处，替换了 onChange 里的 5 处 `as` 断言；改后 RuleEditor 内 `as InterpretationChoice / TimestampOutputFormat / SourceTimeZoneChoice / PatternMode` 0 处；Dialog 的 4 个守卫（`isInterpretationChoice` / `isOutputFormat` / `isTimeZoneKindChoice` / `isSourceTimeZoneChoice`）在基线 `6b0a538aa` 已存在（`:245–257`），fix 只把入参从 `event.target.value` 改为 Select 回调的字符串值 |
| T6 | 文件头原生 select 决定注释 | diff + 全文 grep | ✅ RuleEditor 文件头第 3 点已改写（“下拉框用共享 Select（components/ui/Select，listbox 触发器；data-testid 落在触发器上，选项经 select-option 直测）”）；Dialog 文件头无此类注释；全仓生产代码 `grep "<select"` 0 处（`noNativeSelect` 守卫 G1a 通过即是同一结论的测试级证据） |
| T7 | ui/Select 用法与既有生产用法一致 | 全仓 grep `ui/Select` 生产引用 + `Select.tsx` 源码对照 | ✅ 生产代码 45 个文件使用同一 `components/ui/Select`（value / options / onChange / data-testid / title），T12 的用法与之同款：Dialog 4 处传 `title`（可访问名来源，见 P2a），RuleEditor PatternField 传 `title={label}` + `fitContent`，其余 4 处不传 title（与多数既有调用一致，见 O-2）。前端包装层（`components/ui/Select.tsx`）把 `title` 透传给 `@datazen/ui` 的 Select，后者以 `title ?? placeholder ?? 'Select…'` 作触发器 `aria-label`（`packages/ui/src/Select.tsx:154/196`），并把 `data-testid` 落在触发器 button 上、选项以 `data-testid="select-option"` 经 portal 渲染 |
| T8 | 无行为差异 | 新旧选项清单逐项对照 + P1 探针 | ✅ 选项集合逐项相同（Dialog：INTERPRETATIONS 4 项 / OUTPUT_FORMATS 3 项 / TIME_ZONE_KINDS 3 项 / SOURCE_TIME_ZONE_CHOICES 4 项；RuleEditor：interpretationOptions 原数组、格式 3 项、时区 3 项、源时区 none+3 项、PatternField exact/regex 2 项）；默认值（`value` prop）未动；禁用态未动（旧 select 无 `disabled`，新 Select 亦未传）；onChange 语义未动（Dialog 沿用既有守卫；RuleEditor 的 `as` 断言改为守卫后，无效值由“被接受”变为“被丢弃”，但 ui/Select 只发出 options 内的值，该分支生产上不可达）；校验逻辑未动（P1d 非法 IANA 仍拒存；P1e 全字段规则保存逐字一致；G1b 141/141 通过） |
| T9 | 修复人 `getByText` 歧义说法复核 | P1c 探针 | ✅ 属实：已选值重选时，触发器文本与 listbox 内同 label 选项文本相同，`getByText` 抛多元素歧义；`getAllByTestId('select-option')` 只命中打开的 listbox 内的选项（格式下拉 3 项），重选无歧义。另：仓库 Section 测试既有的 `pickOption`（`getByText` + mouseDown）在目标 label 与当前值不同时仍可用，未被本次改动破坏（G1b 44/44 通过即证） |
| T10 | 旧 id 引用清理 | 全仓 grep 四个旧 id | ✅ `column-presentation-interpretation / -format / -display-timezone / -source-timezone` 四个旧 `id` 在生产代码与测试中 0 处残留引用（测试改用 `data-testid`） |
| T11 | Prettier + 空白检查 | `npx prettier --check`（4 个改动文件）+ `git diff --check 6b0a538aa..fef1ae9fa` | ✅ 通过。Prettier `All matched files use Prettier code style!`；diff-check 无输出 |
| T12 | 作者核实（修复人称首笔提交最初以 flyxl 落盘后已 `--amend --author` 纠正） | `git log --format='%an %ae'` | ✅ 属实：fix `fa4182a2c` 与 test `fef1ae9fa` 作者均为 `issue-32-agent <issue-32-agent@users.noreply.github.com>`；docs `f70709002` 与基线 `6b0a538aa` 亦然。三笔提交的作者、邮箱、提交信息格式均与轨道惯例一致 |
| T13 | 文档事实逐项核对（G7 核心） | 逐条 grep/重算，见下表明细 | ✅ 全部成立，见下表 |

> T13 明细（文档断言 → 代码/测试证据）：
>
> | 文档断言 | 证据 |
> |----------|------|
> | 指南 §14 入口：设置 → 数据浏览 → 时间显示；总开关默认关闭 | `SettingsContent.tsx:441–443`（`activeSection === 'dataBrowsing'`，标题 `t('settings.dataBrowsing')`）；`zh-CN/settings.ts:56`（`'settings.dataBrowsing': '数据浏览'`）；`TableDataSettingsSection.tsx:136`（`<ValuePresentationSettingsSection />` 挂在片段末尾）；`settings.ts:74–78`（默认 `renderers: {}`，即关闭） |
> | 指南 §14.1 三档：跟随规则 / 原始值 / 时间；Unix 秒毫秒不自动猜 | `ColumnPresentationDialog.tsx:190–240`（`draftFromColumnOverride`：BIGINT 默认显式选 Unix 毫秒，注释“不是按位数推断”）；T09 P4/P1b 逐字验证；T12 P1b 独立复核 |
> | 指南 §14.1 总开关关闭时已保存 custom 覆盖仍生效 | `presentationPlan.ts:362` 注释 + `buildOneColumnPlan` custom 分支 bypass enabled 门禁；T05 O-1/T09 P6b 已确立为有意设计 |
> | 指南 §14.1 查询列无来源时「保存为表列覆盖」不可用并说明原因 | `ColumnPresentationDialog.tsx:414`（`source === undefined` 时 `handleSave` 直接返回）、`:504`（`saveDisabled`）、`:768`（禁用说明区）；T09 P7 验证 |
> | 指南 §14.2 双锚点例子逐字 | P3a/P3b 经真实贡献 `prepare/present` 重算逐字命中；另 `date -u` 独立重算：`1705316400` = `2024-01-15T11:00:00Z` = `19:00:00+08:00`，`2026-01-15T14:30:00Z` = `22:30:00+08:00` |
> | 指南 §14.2 优先级：覆盖 > 规则 > 默认；规则命中后不因行解析失败回落 | `presentationPlan.ts:357–380`（raw → custom → 规则首条命中 → 默认路由）；规则 `findFirstMatchingRule` 返回首条后计划即冻结（`:49–64`） |
> | 指南 §14.2 IANA 名首尾空格去掉后保存；不存在的时区名无法保存 | T07-Q1 已裁决接受 trim（T11 报告 O-7）；`validateTimestampOptions` / `isValidIanaTimeZoneName`（`timestampFormat.ts:51`）+ T07/T12 P1d（`Mars/Olympus` 拒存） |
> | 指南 §14.2 夏令时歧义不猜测；无时区文本不隐式解析；DATE/TIME 默认保留 | `temporalMath.ts:131/652/719`（`dst-ambiguous` / `dst-invalid` 拒绝）；T02 P3/P4、T03 P3、T12 P3d；`timestampContribution.ts:292`（无源时区时墙上文本回退 unchanged） |
> | 指南 §14.2 精度：ISO 完整保留微秒/纳秒；locale 最多毫秒 | `timestampFormat.ts:106–121`（`resolveFractionDigits` 最高 9 位）、`:13–16` 文件头（locale 截断到毫秒的决策注释） |
> | 指南 §14.3 规则上限 20 条、模式 128 字符、规则名 64 字符；已存超限只留前 20 | `valuePresentationRules.ts:38/41/44`（`MAX_VALUE_PRESENTATION_RULES = 20` / `MAX_PATTERN_LENGTH = 128` / `MAX_RULE_NAME_LENGTH = 64`）；`:666–674`（解析循环到 20 即停）；P3c 断言常量 |
> | 指南 §14.3 规则至少一个匹配条件；受限正则 flags 只允许空或 `i` | `valuePresentationRules.ts:14–15`（`match` 至少一个字段）、`:405`（flags 只接受 `''` / `'i'`）、`:149/302/306`（反向引用与环视拒绝） |
> | 指南 §14.4 覆盖键结构；缺失记 null 与空串区分；上限 200 条 | `presentationPlan.ts:136–144`（`columnOverrideKey` = `JSON.stringify([connectionId, database ?? null, namespace ?? null, table, column])`）；`columnOverrides.ts:68`（`MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES = 200`）、`:104–122`（`validateStoredColumnOverrides` 解析到 200 条有效条目即停）、`:188–212`（保存超限报 `too-many-overrides`） |
> | 指南 §14.5 / data-table §14.5 同步机制：保存只写改过路径 + 纯通知 + 权威重读 | `settingsStore.ts:326`（发布 `datazen:settings-changed` 纯通知）、`useSettingsSync.ts:21–24`（监听后调 `loadSettings()` 重读）；T11 P1/P6a/P6c 全链路验证 |
> | 指南 §14.5 / data-table §14.5 停用回退：配置保留、重启用恢复、per-JS-context | `registry.ts:56–74`（`unregister` / 句柄身份绑定）；T11 P5/P5-panel 验证 |
> | 指南 §14.5 未知版本原样保留由前端保存链路保证，Rust 结构体无兜底 | `settingsStore.ts` T11-BUG-001 修复链路（未知版本以权威原文作 updater 基底）；`crates/datazen/src/settings/domain.rs:66–77`（`ValuePresentationSettings` 四字段、无 `flatten`；T11 R5 已核实） |
> | data-table §14.1 快照唯一输入；三处提供方；aside 经 ContentViewDrawers 拿同一快照与来源 | `useValuePresentationSnapshot` 三处调用：`PanelContentRenderer.tsx:215`、`ContentView.tsx:93`、`ResultTableView.tsx:101`；`ContentViewDrawers.tsx:317/445–446`（`presentationSource` 与 `presentationSnapshot` 同传 `DetailPanel`） |
> | data-table §14.1 计划缓存键 | `presentationPlan.ts:500–525`（`computePresentationPlanCacheKey`：来源身份、列名/类型/语义、配置修订、贡献修订、locale、localTimeZone、列元数据版本；不含 registry 实例身份） |
> | data-table §14.2 CellRenderer 顺序：编辑态 → NULL → 计划 text → 原有分支 | `CellRenderer.tsx:97`（编辑态）、`:115`（NULL）、`:128`（`presentationPlan?.present`）；T05 P4/P9 验证 |
> | data-table §14.3 NULL 在计划前返回；`other` 列 NULL 得 `not-applicable` | `timestampContribution.ts:268–276`（`other` 门禁在 NULL 检查之前：`other` 列 NULL 返回 `not-applicable` 而非 `null`）；`CellRenderer.tsx:115` |
> | data-table §14.3 aside `<pre>` 可换行、grid 呈示 span CSS truncate、无 120 字 JS 截断 | `DetailPanel.tsx:333/365`（`<pre>`）、T05 P9（呈示 span 含 `truncate`、200 字文本完整保留在 DOM、与 datetime 分支逐类相同）；T05 BUG-005 已关闭（回归验证见 T05 报告 §7） |
> | data-table §14.5 全局配置体不在宿主边界验证；custom 覆盖 options 在宿主边界验证 | `settings.ts:160` 起（`buildValuePresentationSnapshot` 只认版本与存在性）；`presentationPlan.ts:309–323`（custom 分支调贡献 `validateConfig` 校验 override.options，失败为 `invalid-config`） |
> | data-table §14.7 locale 格式 Intl 每格新建 | `timestampFormat.ts:240`（`formatLocale` 内 `new Intl.DateTimeFormat(locale, options)`，无缓存）；prepare 只解析配置与时区 |
> | data-table §14.7 能力名 `data.valuePresentation` 无代码级常量 | `contracts.ts:133` 注释（“由宿主侧声明，不在此重复定义”）；`registry.ts` 等处无该字符串的常量声明 |
> | data-table §14.7 `valuePresentationRegistry` 全局单例；`useValuePresentation` 缓存键不含 registry 实例身份 | `registry.ts:130`（全局单例）；`computePresentationPlanCacheKey` 入参无 registry（见上） |
> | data-table §14.7 规则/覆盖解析超限静默截断，面板保存后超限部分丢掉 | `valuePresentationRules.ts:666–674`（规则到 20 即停）；`columnOverrides.ts:104–122`（覆盖到 200 条有效条目即停） |
> | data-table §14.7 settingsStore / GridCell 函数覆盖率在 T05/T06 验证时恰为 80.0 | T05 报告 `:49/166`（GridCell Funcs 80.0）、T06 报告 `:50/143`（settingsStore Funcs 80.0） |
> | data-table §14.7 hook 文件不在根 vitest 覆盖 include 清单内 | 与 T11 报告 O-12 同类陈述；根 `vitest.config.ts` 的 coverage include 清单未含 `useValuePresentationSnapshot.ts` 等 hook 文件（抽查核实，未逐项展开） |
> | extensibility §1.7 集合注册：重复 ID 显式拒绝；卸载句柄绑定注册实例 | `registry.ts:56–74`（重复抛错“registration never overwrites”；返回的句柄只在 `registrations.get(id) === registration` 时才移除） |
> | extensibility §1.7 首个贡献 `datazen.timestamp` 是核心内置、随启动注册，不是独立 EP 包 | `registerBundledContributions.ts:19/25`（注册核心内置贡献，当前只有 `datazen.timestamp`）；`startSharedApp.tsx:171`（启动时调用 `registerBundledValuePresentationContributions()`，幂等、永不卸载） |
> | extensibility §1.7 pattern 只保存 source 与受限 flags，不持久化 RegExp | `valuePresentationRules.ts:56–62`（`ValuePresentationNamePattern`：`{kind:'exact', value}` / `{kind:'regex', source, flags?}`，注释“绝不持久化 RegExp”） |
> | extensibility §1.7 `PreparedPresentation` 本身无状态字段；不要以 `status === 'ready'` 判断可渲染 | `contracts.ts:55–57`（`PreparedPresentation` 仅 `present(value)` 一个方法） |

## 3. 设计符合性逐条结论

对照设计文档、PRD 与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 文档 3 文件 +185 行（指南 §14、data-table.md §14 与 §13 影响表、extensibility.md §1.7） | ✅ 符合（`git show f70709002 --stat`：指南 +80、data-table +80、extensibility +25，合计 +185/−0；三节内容与设计 §12 要求的三处落点一一对应） |
| 2. 两处按实际交付纠正的措辞：设置入口实际标签是数据浏览 `settings.dataBrowsing`，不是“表格数据”；`datazen.timestamp` 是核心内置贡献随启动注册，不是独立 EP 包 | ✅ 符合（入口标签见 §2.3 T13 第一行；内置注册见 T13 extensibility 行。另注意：PRD §3.1 原文仍写“表格数据”，那是归档的设计提案快照；T12 文档按实际交付写“数据浏览”是正确的纠正。源码侧两处文件头注释仍写“表格数据”，见 O-3） |
| 3. 无英文版浏览指南故无双语同步项 | ✅ 符合（`docs/features/` 下浏览指南仅 `data-browsing-guide.zh-CN.md` 一个文件；T12 未新增英文版，§12 亦不要求新建） |
| 4. fix：两文件 9 处原生 select 改用 `components/ui/Select`（Dialog 4 处、RuleEditor 5 处，含 PatternField mode 选择） | ✅ 符合（§2.3 T5；全仓生产代码 `<select` 0 处） |
| 5. fix：新增 5 个守卫函数替换 onChange 里的 `as` 断言 | ✅ 符合（§2.3 T5；RuleEditor 内相关 `as` 断言 0 处残留） |
| 6. fix：文件头原生 select 决定注释已删除 | ✅ 符合（§2.3 T6；RuleEditor 文件头已改写） |
| 7. fix：测试 helper `pickSelectOption` / `setSelect`（click 触发器 → `getAllByTestId('select-option')` → mouseDown） | ✅ 符合（test 提交 diff 全文核实；P1c 独立复核了该写法的必要性） |
| 8. fix：无断言删除、无期望放宽 | ✅ 符合（§2.3 T4；删除行中 `expect(` 0 处） |
| 9. fix：Dialog 的 label htmlFor 改为同样式 span、可访问名改由 title 传入 | ✅ 符合（diff 核实：4 处 label→span，`className` 不变，各 Select 传 `title`；P2a 验证触发器 `aria-label` 等于 title） |
| 10. fix：守卫通过、定向 142/142、双引擎 typecheck 与命名门禁全过 | ✅ 符合（G1 142/142、G4 双引擎 OK、G5 ok，均由 tester 独立重跑，数字见 §2.1） |

**设计 §12（文档落地）**

| 条目 | 结论 |
|------|------|
| 使用入口、时间语义和例子写入数据浏览指南 | ✅ 符合（指南 §14：入口 §14.1、时间语义 §14.2、双锚点例子逐字，P3a/P3b 复核） |
| 共享展示管线与原值不变量写入 DataTable 架构 | ✅ 符合（data-table.md §14：管线 §14.1、决策顺序 §14.2、原值不变量 §14.3、覆盖键 §14.4、配置模型 §14.5、同步 §14.6、已知局限 §14.7；事实逐项见 §2.3 T13） |
| 按实际交付更新扩展开发说明 | ✅ 符合（extensibility.md §1.7：集合注册契约、与单实现 EP 的区别、内置贡献说明、职责划分、停用回退；两处纠正措辞见上表第 2 条） |
| 本文件（设计文档）保留设计历史，不冒充持续维护的实现事实 | ➖ 不在 T12 改动范围：设计文档本身未动 |

**设计 §10-C（规则与双端恢复，与 T12 相关项）**

| 条目 | 结论 |
|------|------|
| 规则编辑、稳定顺序、正则约束 | ✅ 符合（T08；T12 文档 §14.3 复述准确，见 T13） |
| 跨窗口同步、来源隔离、异步过期响应与查询结果来源降级 | ✅ 符合（T10/T11；T12 文档 §14.4–§14.6 复述准确） |
| 完整 E2E：开启 → 表头设 Unix 毫秒 → 检查 cell 与 aside → aside 改时区 → 编辑仍为整数 → 保存/重开 → 切同名其他表 → 清除覆盖跟随规则 → 调整规则顺序 → 停用/重新启用扩展；额外覆盖分屏与查询结果切换 | ⚠️ 未交付：`e2e/specs/` 下无 valuePresentation 旅程 spec（P5 核实），且沙箱内 Tauri webdriver 构建无法链接（见 §7）。T12 的全量回归是 vitest 层（G2/G3），不是该 E2E 旅程。文档未宣称 E2E 已交付，不构成虚假陈述；属残余缺口，见 O-5 |

**PRD §6 验收 1–12（对照 T01–T11 报告与本次验证）**

| # | 验收 | 结论 |
|---|------|------|
| 1 | 新安装或未启用时，现有显示与编辑行为保持不变 | ✅ 符合（T05 P3 默认关闭；T07；G2 全量 9538 通过无回归） |
| 2 | 两个入口修改的是同一配置；cell、aside、预览显示结果一致 | ✅ 符合（T09 P1 三方一致；T05 P1/P2；T12 P1/P3a/P3b 双锚点逐字） |
| 3 | BIGINT 无显式配置时保持数字；配置 Unix 秒/毫秒后按指定单位展示 | ✅ 符合（T09 P4；T12 P1b 独立复核秒/毫秒显式区分） |
| 4 | 列覆盖优先于规则；「原始值」阻断规则；清除覆盖恢复继承 | ✅ 符合（保存语义 T09 P1c/P2/P3、T10；T09-BUG-001 的预览缺陷已修复并回归验证，见 T09 报告 §8） |
| 5 | 规则顺序可调整，禁用规则不参与匹配，错误规则不能保存 | ✅ 符合（T08；`findFirstMatchingRule` 跳过 `enabled: false`，T12 源码复核；P1e 全字段规则保存） |
| 6 | 重启恢复配置；不同连接、数据库、schema 的同名表不串配置 | ✅ 符合（T06 重启恢复；T10 来源隔离与 200 上限；T12 §2.3 T13 覆盖键行） |
| 7 | 切换选中行、分屏、切换查询结果集时，aside 始终使用正确来源的格式计划 | ⚠️ 部分符合：同一面板内两入口共用同一快照与来源已验证（T09；T12 §2.3 T13 ContentViewDrawers 行）；**行切换/分屏/结果集切换的整页回归未覆盖**（T09 报告 §3 明确属 T12 范围；T12 未补测试，文档 §14 亦未明示该缺口）。文档未宣称验收 7 已全覆盖，见 O-5 |
| 8 | 修改时区、源时区与单位能正确跨日；无时区值不发生隐式漂移 | ⚠️ 大体符合：跨日已验证（T07 P1d `2026-01-15T20:30:00Z` → `2026-01-16T04:30:00+08:00`）；DST 歧义/不存在在贡献层已验证（T02 P3/P4、T03 P3、T12 P3d）；**设置预览的 DST 切换日用例未覆盖**（T07 O-7；T12 未补）。指南 §14.2 的 DST 政策陈述本身属实（核心层行为），见 O-6 |
| 9 | 编辑、复制、排序、过滤、导出仍使用原值；格式变化不产生脏记录 | ✅ 符合（T05 P4/P5 编辑原值与取消往返、T4 QuickLook 仍传原值、G6-T3 分支未动；呈示只改显示文本，原值不动是架构级不变量，data-table §14.3。复制/排序/过滤/导出的端到端旅程未单独立项，属 P5 的 E2E 缺失范围，见 O-5） |
| 10 | 配置保存失败、扩展缺失、格式化异常均可恢复，不阻断网格交互 | ✅ 符合（T11 P10 保存失败恢复、P5 扩展缺失回退、格式化异常由贡献层 `unchanged` 兜底） |
| 11 | Desktop 与 Web 的保存恢复和用户隔离均验证；不能仅凭桌面 extra 字段可保存就宣称双端完成 | ⚠️ 部分符合：Desktop 路径已验证（T06）；Web 侧单元层在 G2 全量中通过（含 web transport / backend-client 测试），但**无端到端双端旅程**（P5）；Rust 侧未知顶层字段不保留的局限已由 T11 R5 证实并在 T12 文档中明示（指南 §14.5、data-table §14.5）。文档未宣称双端 E2E 已完成，见 O-5 |
| 12 | 注册、停用、重新启用时间贡献后，两处同步更新且无旧缓存残留 | ✅ 符合（T11 P5：注册/停用/重启用三态下 grid 与 aside 均同步，停用期间保存的新规则胜出，无旧缓存残留） |

## 4. Bug 列表

**本次验证未发现 bug。**

既有门禁 G1、G3–G8 全部通过；G2 的唯一失败是环境原因的超时（见 §7），不是代码缺陷；对抗探针 P1–P5 全部通过；文档事实逐项核对全部成立（§2.3 T13）。无新增 §4 条目。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（G2 唯一失败是环境超时，已证实，非代码缺陷）**：见 §7。失败模式为 10s 超时（非断言），同一文件独立重跑 36/36 两次通过（默认超时 4.69s、放宽超时 7.64s）；T12 的 diff 与该测试的输入（已提交的 Rust 驱动 crate + cargo metadata）零重叠。另：本次 G2 总数 9538/1/1 与 coder 首轮“修正后实情”数字相同，但失败项不同——首轮失败的是 noNativeSelect（真实 T12 缺陷，已由 fix 修复，本次在 G1a 与全量中均通过），本次失败的是无关脚本测试的超时。不要把两次的“1 失败”混为同一问题。
- **O-2（RuleEditor 4 个 SettingRow Select 只有通用 placeholder 可访问名）**：P2c 实测：interpretation / format / display-timezone / source-timezone 四个 Select 的 `aria-label` 均为翻译后的 placeholder（en 为 `Select…`）。原因有二：`SettingRow` 的 label 是普通 span（非 `<label>`，`settingsUi.tsx:21–30`），无 htmlFor 关联；这 4 处未传 `title`，ui/Select 回落到 placeholder。改动前原生 select 在同一位置同样没有可访问名（无 aria-label、无关联 label），故不构成回归；但与 PatternField（传 `title={label}`，P2b 验证有字段级可访问名）相比不对称。建议后续给这 4 处补 `title`（与 Dialog 的 4 处一致），属无障碍打磨，非缺陷。
- **O-3（两处源码文件头注释仍写旧入口名“表格数据”）**：`ValuePresentationSettingsSection.tsx:2` 与其测试文件 `:2` 的文件头仍写“设置 → 表格数据 → 时间显示”，而实际 UI 标签是 `settings.dataBrowsing` = “数据浏览”（`SettingsContent.tsx:441–443`），T12 文档已按实际交付纠正为“数据浏览”。另 PRD §3.1（归档文件）原文亦为“表格数据”，那是设计提案快照，留档无妨。建议后续顺手改两处源码注释（不改行为），避免后人按注释找入口。
- **O-4（test 提交“prettier 换行”说法无独立改动可辨）**：`git show -w fef1ae9fa` 与原 diff 行数一致（96 行），无纯空白改动；新增的 2 个空行属于两个 helper 插入自带的格式。修复人称 amend 时“仅并入 hook 留下的 prettier 换行”——作为独立改动无法从 diff 中辨认，但其实质主张（test 提交无生产改动、无无关改动）成立（§2.3 T3/T4），不影响结论。
- **O-5（E2E 层残余缺口：验收 7 的整页回归、验收 9/11 的端到端旅程、设计 §10-C 的完整 E2E 均未交付）**：`e2e/specs/` 下无 valuePresentation 旅程 spec（P5 全量清单核实）；沙箱内 Tauri webdriver 构建无法链接（见 §7）。T12 的全量回归是 vitest 层（G2 9538 通过），覆盖了单元与集成级行为；行切换/分屏/结果集切换的整页回归、复制/排序/过滤/导出的端到端旅程、双端保存恢复旅程仍缺。T12 文档未宣称 E2E 已交付（§3 表格如实标注部分符合），故属未覆盖范围而非虚假陈述；是否补 E2E 由协调方决定（需真 Tauri 运行时或 CI）。
- **O-6（验收 8 的 DST 设置预览用例仍未覆盖）**：T07 O-7 建议 T12 补一条 DST 切换日预览用例，T12 未补。贡献层的 DST 拒绝行为本身已充分验证（T02 P3/P4、T03 P3、T12 P3d：`2024-11-03 01:30` America/New_York → `dst-ambiguous`），指南 §14.2 的政策陈述属实；缺的只是设置面板预览在 DST 切换日的端到端用例。建议后续任务补一条（经 `computeTimestampPreview` 的 DST 预览断言即可，无需新脚手架）。
- **O-7（locale key 完整性：无 vitest 测试，但有脚本级检查且 0 缺失）**：T07 O-9 称“无 locale key 完整性测试”——准确的说法是无 vitest 测试；仓库已有脚本级检查 `scripts/i18n-sync-check.mjs`（release 工作流运行；其 vitest 包装 `i18n-sync-check.test.mjs` 30 项在 G2 全量中通过），且本次独立核查：en 与 zh-CN 的 `settings.valuePresentation*` key 各 75 个、集合完全一致、0 缺失；`node scripts/i18n-sync-check.mjs` 输出 0 missing、7 stale（见 O-11）。T07 O-9 的缺口已比原陈述窄：缺的是持续断言测试，不是检查本身。
- **O-8（探针吸收未落地）**：T01–T11 各报告建议吸收的探针用例（含 T11 O-15 的 P1/P2b/P3/P6a/P6c/P8/P9 清单）均未在 T12 落地为仓库回归测试；T12 本身新增的探针（P1–P5）同样是一次性验证资产，已按惯例删除。建议后续任务从中挑高价值者吸收——优先 P1c（已选值重选不歧义）、P3d（DST 双义拒绝经贡献层）、P2a/P2b（可访问名）。
- **O-9（“不可变快照”是类型级 readonly，与设计用语一致）**：data-table §14.1 称快照“不可变”；核实：`ValuePresentationSnapshot` 各字段均为 `readonly` 类型，但构建函数未做运行时 `Object.freeze`（settings.ts / presentationPlan.ts 全文件 grep `Object.freeze` 0 处）。设计文档 `:151` 原文即为“验证后以不可变快照传入 prepare”，T12 文档沿用设计用语，不算文档错误；此处特此注明，避免读者误以为运行时冻结。
- **O-10（G2 耗时 35 分钟，长于 coder 首轮的约 30 分钟）**：本次全量与 G1/G3/G4/G6/探针在同一台 1.5 核共享机上并行跑，互相争抢 CPU（G3 独占 520s、typecheck 两引擎、cargo 编译均在 G2 进行期间跑过）。后续全量重跑建议独占机器跑，或至少避开其他重型任务；本次 G2 的超时失败即是同一争抢的后果（见 §7）。
- **O-11（`i18n-sync-check` 退出码 1：7 个 stale 翻译，既有问题）**：`node scripts/i18n-sync-check.mjs` 退出码 1，0 missing、7 stale（`chat.mode.agent`、`ai.write.hash`、`mcp.http.token`、`mcp.approval.sql`、`filter.modeRawSql`、`filter.rawBadge`、`settings.valuePresentation.timeZone.utc`）。其中 `timeZone.utc` 的 zh 值 `UTC` 与 en 相同，属语言中性（T07 P10 已接受）；其余 6 项与 valuePresentation 无关。7 项均为 T12 之前已存在（T12 未改任何 locale 文件），不阻塞 T12；stale 清单由 i18n 专项处理。
- **O-12（测试 stderr 的 `common.close` 缺译警告）**：与 T09 O-7 / T10 O-7 / T11 O-13 同一既有模式（`@datazen/ui` 自身注册表，单元测试环境未注册该 key），非 T12 引入；不影响测试结果（G2 汇总以退出码与计数为准）。
- **O-13（桌面 Rust 测试与 Tauri webdriver 构建均受同一环境限制）**：G6c 桌面 `cargo test --lib` 链接失败（缺 `-lwebkit2gtk-4.1` / `-lgtk-3` / `-lgdk-3` / `-lgdk_pixbuf-2.0`）与 P5 的 webdriver 构建缺失（`pkg-config` 缺 `webkit2gtk-4.1` / `gtk+-3.0`）是同一根因：沙箱无系统 GTK/WebKit 库。不要装系统依赖；留给 CI。T12 无 Rust 改动，该限制不影响 T12 结论。
- **O-14（P1c 复核了修复人的 `getByText` 歧义说法）**：已选值重选时 `getByText` 确实抛多元素歧义，`select-option` 只命中打开的 listbox——修复人选择 `getAllByTestId('select-option')` 的理由成立。仓库 Section 测试既有的 `pickOption`（`getByText` + mouseDown）在目标 label 与当前值不同时仍可用，未被本次改动破坏。
- **O-15（探针资产与回归建议）**：`~/workspace/t12-probe` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；P4 链接脚本亦已删除，方法见 §2.2 P4；仓库 `git status` 复核干净，除本报告外无新增文件）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t12-p-*.log`、`~/workspace/t12-p12.log`、`~/workspace/t12-p2.log`、`~/workspace/t12-p3.log` 与 `~/workspace/t12-g*.log`。建议将 P1a/P1b（格式与单位切换）、P1c（已选值重选）、P1e（RuleEditor 全字段保存）、P2a/P2b（可访问名）、P3a/P3b/P3d（双锚点 + DST）吸收为仓库回归测试；P4 的链接检查可纳入文档门禁脚本。

## 6. 结论

**通过，可做收尾提交。**

- 既有门禁：守卫与定向 142/142（守卫 1 + 定向 141：Dialog 25、Entries 2、Section 44、valuePresentationRules 70）、严格模式抽查 98 文件 1182/1182、双引擎类型门禁通过（tsrs / tsc 7.0.2）、命名门禁通过、Rust settings 57/57、desktop check 与 fmt 退出码 0；三提交无任何 Rust 改动；Prettier 与 diff-check 全过；与 coder 申报逐项一致（G1 142、G3 1182、G6 57、作者核实）。
- 全量重跑：855 文件，9538 通过 / 1 失败 / 1 跳过（9540；耗时 2127s）。唯一失败是 `check-driver-type-isolation` 的 10s 超时，已证实为环境原因（独立重跑 36/36 两次通过：默认超时 4.69s、放宽超时 7.64s；T12 diff 与其输入零重叠；见 §7）。coder 首轮的真实失败 noNativeSelect 本次在守卫与全量中均通过。1 跳过是 `cellEditorCandidates.test.ts` 的既有 1 项，与 T12 无关。
- 对抗性探针 12 项 vitest 探针 + 16 项链接检查全部通过：P1 ui/Select 行为 5/5（格式切换、秒/毫秒切换、已选值重选不歧义、非法 IANA 校验不变、RuleEditor 全字段保存）、P2 可访问性 3/3（Dialog 4 触发器 aria-label、PatternField aria-label 保留、SettingRow 通用名现状）、P3 文档事实 4/4（双锚点逐字、上限常量、DST 拒绝）、P4 链接 16/16、P5 E2E 缺失属实（无 spec、沙箱无法链接）。
- 设计符合性：设计 §12 的三处文档落点均交付；coder 申报 10 条逐项核实成立；PRD §6 验收 1–6、9、10、12 符合，7/8/11 部分符合（残余缺口：整页挂载回归、DST 设置预览、双端 E2E 旅程——文档未谎称全覆盖，见 §3 与 O-5/O-6）；设计 §10-C 的完整 E2E 旅程未交付（P5），属已知未覆盖范围。
- 本次验证**未发现 bug**（§4）。观察项 15 条均为非缺陷（O-1 G2 环境超时、O-2 SettingRow 通用可访问名、O-3 源码注释旧入口名、O-4 prettier 说法、O-5/O-6 验收残余、O-7 locale 检查形态、O-8 探针吸收、O-9 快照不可变用语、O-10 全量耗时、O-11 stale 翻译、O-12 `common.close` 警告、O-13 桌面链接限制、O-14 `getByText` 歧义复核、O-15 探针资产）。
- 工作区复核：报告写完后 `git status` 除本报告（未跟踪新文件）外干净；tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：G2 唯一失败的环境原因分析与 P5 E2E 缺失依据

### 7.1 G2 唯一失败：`check-driver-type-isolation` 10s 超时（环境原因，已证实）

**现象**：G2 全量（`npx -y pnpm@12.18.0 test`，开始 06:57:42 UTC，耗时 2127.13s）汇总为 855 文件、9538 通过 / 1 失败 / 1 跳过。唯一失败：

```
FAIL scripts/__tests__/check-driver-type-isolation.test.ts > this repository
  > has no cross-driver type edge in the committed tree
Error: Test timed out in 10000ms.
```

该失败出现 3 次（初次 + retry x2，均超时 10s）；文件级耗时 68906ms。失败是**超时**，不是断言失败——检查本身未完成，`expect(result.violations).toEqual([])` 从未执行，没有发现任何跨驱动类型边。

**为什么是环境原因（四点证据）**：

1. **失败模式是超时，不是违规**。测试注释自述（`:663–668`）：“The one test here that is not synthetic. It costs a real `cargo metadata` call”。`cargo metadata` 在 15 个驱动的 workspace 上耗时远超 10s 是常态——本次独立重跑中该文件总耗时 4.69s/7.64s（含 35 个快速合成用例），其中 cargo metadata 调用占大头；在 G2 全量期间机器同时跑着 G3（98 文件、520s）、双引擎 typecheck、cargo test/check 编译与探针 vitest，1.5 核共享 CPU 被 starve 到 10s 内完不成一次 metadata 调用。vitest 配置文件头亦有同类记载：重载下单测最坏 11629ms 已超 10s（`vitest.config.ts` 注释）。
2. **独立重跑两次全部通过**。G2 结束后、机器空闲时：`npx vitest run scripts/__tests__/check-driver-type-isolation.test.ts --retry=0`（默认 10s 超时）→ **36/36 通过，4.69s，退出码 0**（日志 `~/workspace/t12-g2c-isolation-default-timeout.log`）；放宽 `--testTimeout=180000` 再跑一次 → **36/36 通过，7.64s，退出码 0**（日志 `~/workspace/t12-g2b-isolation.log`）。
3. **T12 的 diff 与该测试的输入零重叠**。该测试分析的是已提交的 Rust 驱动 crate（`crates/drivers/*` + cargo metadata）；T12 三笔提交共 7 个文件，全部是文档（`.md`）与前端（`.tsx`），`.rs` 0 处、scripts 0 处——其判定结果在基线 `6b0a538aa` 与 HEAD `fef1ae9fa` 下不可能不同。
4. **总数巧合的澄清**。本次 9538/1/1 与 coder 首轮“修正后实情”数字相同，但失败项不同：首轮是 noNativeSelect（真实 T12 缺陷，已修复，本次在 G1a 与全量中均通过）；本次是无关脚本测试的超时。两次的“1 失败”不可混为同一问题。

**结论**：G2 的 1 失败不构成 T12 的代码回归；按任务规则（“除环境原因外的失败”才阻塞通过），不影响 §6 的通过结论。如协调方希望 G2 汇总行字面全绿，可在空闲机器上重跑一次全量（按本节证据，预期通过）；本次不再重跑（35 分钟），以独立重跑证据为准。

### 7.2 P5：E2E 缺失的依据

- **无旅程 spec**：`e2e/specs/` 全量清单 grep `value|presentation|timestamp|time-display`（不区分大小写）0 处；目录其余 spec 均为连接、AI、备份、图表等旅程，无时间显示相关文件。
- **沙箱内 Tauri webdriver 构建确实无法链接**：`pkg-config --exists webkit2gtk-4.1` 缺失、`pkg-config --exists gtk+-3.0` 缺失；G6c 桌面 `cargo test --lib` 链接失败同根因（`rust-lld: error: unable to find library -lwebkit2gtk-4.1 / -lgtk-3 / -lgdk-3 / -lgdk_pixbuf-2.0`）。`pnpm e2e` / `pnpm tauri:build:webdriver` 的构建流程在沙箱内走不到运行 E2E 的步骤。
- **后果**：设计 §10-C 的完整 E2E 旅程（开启 → 表头设 Unix 毫秒 → 检查 cell 与 aside → aside 改时区 → 编辑仍为整数 → 保存/重开 → 切同名其他表 → 清除覆盖跟随规则 → 调整规则顺序 → 停用/重新启用扩展；另加分屏与查询结果切换）未在 T12 交付；PRD 验收 7/9/11 的端到端部分亦然。以上均留给 CI 或有真 Tauri 运行时的环境；T12 的 vitest 全量（9538 通过）是单元与集成级回归，不替代 E2E。
