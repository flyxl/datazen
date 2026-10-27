# 检查命令、证据记录与发布标准（原 §13–§14）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §13、§14 的全部内容；文档权威与阅读路线见总纲 §1。

## 13. 检查命令与证据记录

### 13.1 先确认环境与运行对象

在对应 worktree 根目录运行。安装/生成流程遵循 package.json 和[worktree 隔离规则](../worktree-isolation.md)，不要为了修缺失 codegen 去手写生成文件；driver stash 陈旧时按错误给出的恢复流程处理，不运行旧 stash restore 覆盖合并结果。

每轮检查首尾记录 HEAD、工作区内容指纹和 Git 状态，证明测试的是同一份代码。`git diff --binary HEAD | git hash-object --stdin` 只能覆盖 tracked 差异；新增文件、影响构建的生成输入需另记校验值，不把它谎称为完整工作区指纹，不读取受保护 env 内容。

测试/门禁输出放系统临时目录。下面是可在 Bash 定义的通用包装器，避免 Linux/macOS 的 mktemp 模板差异：

```bash
run_check() {
  local log rc
  log="$(mktemp "${TMPDIR:-/tmp}/datazen-ddd.XXXXXX")" || return 1
  if "$@" >"$log" 2>&1; then
    rc=0
  else
    rc=$?
  fi
  printf 'EXIT=%s LOG=%s\n' "$rc" "$log"
  return "$rc"
}
```

每次运行后检查退出码与日志的实际汇总。确认日志无敏感信息后，可用 `tail -n 40 "$LOG"` 查看结论，其中 LOG 设置为上一步打印的路径。不要把全部 stdout、原始连接串或 panic 中的凭据贴进 PR/对话。

失败后停止该验收结论、先读错误再处理；不能忽略包装器的非零返回。超时、OOM、测试断言失败分别记录。Rust 测试过滤器不是正则 OR，不能用 `a|b` 后看到 0 tests 就报通过。

### 13.2 当前仓库已有的检查

以下命令已从本次基线的 package.json / 脚本核对存在；列出不代表本次文档修改已全部运行。按改动范围先定向再扩大：

```bash
run_check git diff --check
run_check cargo fmt --all -- --check
run_check node scripts/check-ddd-layers.mjs
run_check node scripts/check-platform-crate-boundaries.mjs
run_check node scripts/check-module-layers.mjs
run_check node scripts/check-id-terminology.mjs
run_check node scripts/check-shared-crate-panic-policy.mjs
run_check node scripts/run-platform-crate-tests.mjs --dry-run
run_check cargo test -p datazen_desktop --no-default-features --features contracts,application --lib
run_check cargo test -p datazen_desktop --all-features
run_check cargo test -p datazen_desktop --lib
run_check pnpm typecheck
run_check pnpm exec vitest run src/platform/__tests__/tauriBackendTransport.tableData.test.ts
run_check pnpm exec vitest run src/stores/__tests__/tableDataStore.keyset.test.ts
run_check pnpm exec vitest run src/stores/__tests__/tableDataStore.countStrategyContract.test.tsx
```

当前 `check-module-layers.mjs` 主要检查前端层，不是未来 Rust 功能模块守卫；D1 必须实际扩展 Rust 检查，不能借同一个名字假称已覆盖。当前 DDD 脚本里的固定成员下限与旧包断言也要随合包替换成非空/成员完整性规则，不把合法合包当违规，也不直接删除防漏扫能力。

Rust 链接确属内存不足时，可以用 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 重跑原测试并记录；这不替代最终发布配置检查，也不能掩盖测试失败。

### 13.3 modules / web 建立后的新增验证

**modules 已是当前共享业务 crate；契约 bundle 使用 `contracts` feature，不能再创建名为 `platform_api` 的 crate feature。** web 宿主相关命令随 D7 移出本次范围，桌面范围内的 modules 矩阵照常执行：

```bash
run_check cargo check -p datazen_desktop --no-default-features
run_check cargo test -p datazen_desktop --no-default-features --features settings
run_check cargo test -p datazen_desktop --no-default-features --features connection
run_check cargo test -p datazen_desktop --no-default-features --features monitor
run_check cargo test -p datazen_desktop --no-default-features --features table-browsing
run_check cargo test -p datazen_desktop --all-features
# 以下两条随 D7 延后（web 宿主不在本次范围）
run_check cargo test -p datazen-web
run_check cargo build -p datazen-web --release
```

另外单独执行 Desktop 和 Web 的实际声明 feature 集合并检查依赖闭包。新模块测试包含 lib、集成、文档测试和必要的 release-profile 套件；不能把旧 runtime 集成测试搬丢后只跑 `--lib`。

新旧兼容期同时跑已迁移模块与仍存在的旧包测试。旧包删除的同一个 PR 替换对应 CI 命令，不保留永远报 package-not-found 的步骤，也不无声跳过。

### 13.4 合并与发布检查

驱动注入/生成状态正确后，受影响 Rust 包与宿主检查完整运行；全量基础检查包括：

```bash
run_check cargo check --workspace --all-targets
run_check pnpm test:platform-crates
run_check pnpm typecheck
run_check pnpm exec vitest run
run_check pnpm test:unit:drivers
run_check pnpm build
```

构建/Driver API 或布局变化需要真实驱动 Rust 测试；驱动 live 环境缺失只能记录未完成，不能用 fake 替代发布验收。独立 Pro 扩展的门禁在其自己的仓库运行，宿主绿灯不代表 Pro 通过。

E2E 必须使用项目构建器和匹配的驱动集合：

```bash
run_check pnpm tauri:build:webdriver
run_check pnpm e2e:skip-build
run_check pnpm e2e:contract:matrix
```

不使用裸 cargo build 代替 Tauri Webdriver 构建。迁移三件套若仅 DMG 阶段失败但已生成可运行 `.app`，按项目规则继续验证应用与 WDIO，分别报告功能结果和打包失败。

正式发布还须通过实际 release workflow 的门禁、目标 OS/架构/驱动/Community-Pro 变体构建及安装冒烟。至少包含翻译硬门禁，不因“只是重构”跳过：

```bash
run_check node scripts/i18n-sync-check.mjs
run_check pnpm tauri:build:community
```

Pro 发行仅在对应独立扩展已装配并授权的环境运行其构建和验收。签名、上传、部署、版本/tag/发布动作另需明确授权，完成本方案实现不自动授权这些外部动作。

## 14. 发布标准与最终交付清单

### 14.1 三种状态不要混写

- **文档完成**：目标设计、迁移步骤、检查和链接一致；不等于代码实现。
- **阶段可合并**：本阶段代码与相关检查完成，其余兼容路径有明确下一阶段；不等于整个重构结束。
- **版本可发布**：该版本声明的功能、宿主、驱动与发行变体全部满足真实发布门槛。关键测试未跑、超时或环境缺失时仍有缺口，不能标发布完成。

### 14.2 全部重构完成的必要条件

- [ ] 业务按功能进入一个 modules crate；每个功能拥有应用层与领域层。
- [ ] 独立 application/platform-api/runtime/kernel/modules 与已合入的旧功能 crate 退役；明确保留的插件契约和基础设施包不被误删。
- [ ] Repository trait 由各领域拥有，生产持久化实现在 src-tauri / web；没有全局仓储全集或另一个总 application 层。
- [ ] runtime 私有地归属 execution；Job、Schema cache 等归属对应功能；不存在每个消费者一份资源账本。
- [ ] Controller 只进入应用服务；宿主基础设施与装配可以引用必要领域契约，不被错误的层门禁阻挡。
- [ ] Desktop 能独立选择、装配和编译自己的功能集合；未启用功能无需假依赖。Web 侧随 D7 延后（2026-10-26 决策），不宣称双宿主完成，也不因此放松共享层的形态中立要求。
- [ ] IPC / wire DTO / error / Driver 协议与存储兼容有测试证据；任何行为变化已单独说明和验收。
- [ ] owner/scope/epoch/revision、事务、取消、幂等、资源清理及计划恢复语义没有因移动而削弱。
- [ ] [§12](09-acceptance-tests.md)验收要求都有真实测试映射，最小 feature、全功能、真实宿主和驱动测试没有漏收集。
- [ ] 依赖与模块门禁在合成违规/合法样例上有鉴别力；临时兼容例外清零。
- [ ] D7 已移出本次范围（2026-10-26 决策）：本次交付明确标注为**桌面单宿主模块化版本**，Web 接缝延后到后续版本验收，不宣称双宿主方案完成。
- [ ] 正式架构文档逐节改成实现事实，索引与代码路径可达；阶段台账在合并收尾删除。
- [ ] 发布矩阵、安装/升级与回滚演练符合所声明版本；没有未解释的失败和未运行检查。

### 14.3 回滚演练和发布说明

至少用合成用户数据完成：旧版读写 → 新版启动并执行代表性操作 → 退出/重启 → 兼容允许范围内回退读取。确认配置、凭据引用、历史和计划不丢失，运行时句柄没有被误恢复。

发布说明写用户可感知结果与真实兼容边界，不把“减少 crate 数”“新增几个 trait”当成功标准。无法验证的 OS、驱动、Web 能力和恢复场景明确列出，不能以文档中的计划替代实际证据。
