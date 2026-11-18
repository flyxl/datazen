# Segment 机器执行手册（可直接照做）

> 给领了 segment 1/2/3/4 的机器（人或 agent）用：从零开工到把 task 做完 push。
> 方案背景见 [multi-machine-sharding.md](multi-machine-sharding.md)；本手册只讲「怎么做」。
> 4 台机器各做一段，互不写对方文件；本机协调者统一汇总合流。

## 0. 你领到的是哪一段

| 段 | checkout 分支 | 状态文件（你唯一能写的台账） |
| --: | --- | --- |
| 1 | `feat/rune-native-ui-migration-segment-1` | `docs/development/rune-migration/shards/segment-1.md` |
| 2 | `feat/rune-native-ui-migration-segment-2` | `shards/segment-2.md` |
| 3 | `feat/rune-native-ui-migration-segment-3` | `shards/segment-3.md` |
| 4 | `feat/rune-native-ui-migration-segment-4` | `shards/segment-4.md` |

**永远不要写** `docs/development/rune-migration/tasklist.md`（那是协调者的，你写了会冲突）。

## 1. 一次性准备

```bash
REPO=/你的路径/datazen
cd "$REPO"

# 1.1 拉分支
git fetch origin
BR=feat/rune-native-ui-migration-segment-N   # N 换成你的段号
git checkout "$BR"
git pull --ff-only origin "$BR"

# 1.2 Rust 工具链（项目要求 stable ≥1.100；Rune web 另需 1.99.1，本任务只需 stable）
rustup toolchain install stable            # 确认 rustc --version ≥ 1.100.0
rustup target add wasm32-unknown-unknown --toolchain stable
# 若只做原生 Rune UI（不需要 web 预览），wasm target 可省。

# 1.3 子模块（必须，否则 cargo 找不到 vendor/rune）
git submodule update --init --recursive vendor/rune

# 1.4 node_modules 是主检出的软链，不要 pnpm install；二进制用 npx。

# 1.5 初始化本机串行编译队列（脚本已在仓库里）
export RUNE_BUILDQ="${TMPDIR:-/tmp}/rune-buildq"
mkdir -p "$RUNE_BUILDQ"
# 之后每开一个 worktree coder/tester，就往 bindings 加一行 requester<TAB>workdir，见 §3.2。
chmod +x "$REPO/scripts/rune-buildq/"*.sh
nohup bash "$REPO/scripts/rune-buildq/worker.sh" > "$RUNE_BUILDQ/worker.out" 2>&1 &
sleep 2
cat "$RUNE_BUILDQ/heartbeat"   # 每秒推进的时间戳 = worker 活着
```

## 2. 每天开工：看本段下一个任务

```bash
node scripts/rune-parity.mjs --shard N --next-todo | head -5
# 输出 W1 ... T### area days react files —— 取第一个（编号最小）
```
`--shard N` 列出全段，`--next-todo` 只列本段未完成的。状态来自你的 `shards/segment-N.md`。

## 3. 做一个 task（完整循环）

### 3.1 开 worktree

```bash
TRACK=rune-a1                      # 本 task 独占的轨道名，自己起，别和别人重
scripts/new-feature-worktree.sh "$TRACK" "$BR"
git -C ".worktrees/datazen-$TRACK" submodule update --init --recursive vendor/rune
```

### 3.2 给队列加绑定（让编译只发生在本 worktree）

```bash
printf '%s\t%s\n' "$TRACK" "$REPO/.worktrees/datazen-$TRACK" >> "$RUNE_BUILDQ/bindings"
# 若用 tester，也加一行 "<TRACK>-audit<TAB>同 workdir"
```

### 3.3 Coder：port 组件

- 必读：`AGENTS.md` + `docs/development/rune-migration/tasks-react-parity.md` §1/§2/§5 + `progress.md` + 该 React 源 + 已 port 的同类参考（见 `crates/datazen-ui/src/components/`）。
- Rune 文件落在与 React 同名路径；一 task 一提交 `feat(rune-ui): port <react path>`。
- **所有编译/测试走串行队列**，禁止直接 cargo：

```bash
bash "$REPO/scripts/rune-buildq/submit.sh" "$TRACK" "$PWD" \
  'CARGO_TARGET_DIR=$PWD/target bash scripts/rune-migrate-gate.sh'
# 末行必须是 gate: PASS
```

- 提交纪律：`git commit --no-verify`；只 `git add` 自己的文件；禁 `git add -A`；禁 `cargo fmt --all`；禁 push 主分支。

### 3.4 Tester：独立审计（必须，不能复用 coder）

- 新开 tester（同样的 worktree 绑定），只测不修；复现 coder 数字 + 变异反证（临时改坏实现 → 测试必须变红 → 贴失败原文 → 还原转绿）。
- 同样走 `submit.sh`（requester 用 `<TRACK>-audit`）。
- 判 `TEST_FAILED` → 把缺陷清单交回 coder 修（最多 5 轮）；`TEST_PASSED` → 继续。

### 3.5 合流到你的 segment 分支并标记状态

```bash
cd "$REPO"                         # 回主检出（当前分支是你的 segment 分支）
git merge --no-ff "feature/$TRACK" -m "feat: merge $TRACK"

# 翻本段状态（只写你自己的 shard 文件！跨段 T 会被脚本拒绝）
node scripts/rune-parity.mjs --shard-status T###=done --shard N

git add docs/development/rune-migration/shards/segment-N.md
git commit --no-verify -m "docs(rune): mark T### done (segment N)"
git push origin "$BR"              # 合流即 push（用户要求）
```

### 3.6 清理本 task 资源（磁盘）

```bash
git worktree remove --force ".worktrees/datazen-$TRACK"
git branch -d "feature/$TRACK"
```

然后回到 §2 取下一个任务。

## 4. 纪律速查

- 编译测试**全部**经 `submit.sh` 串行队列；不直接 cargo；job 在途不改源码。
- 只写自己的 `shards/segment-N.md` 和自己的 segment 分支；不碰主 tasklist、不碰别人的段。
- `git commit --no-verify`；只 add 自己的文件；禁 `git add -A` / `cargo fmt --all`。
- 只 push 自己的 segment 分支；不建 PR / 不发版 / 不部署。
- Rune 文件里禁裸 `unwrap()`/`expect()`；禁新 `oklch()` 字面量（palette.rs 除外）；图标复用 `icons.rs` 注册表。
- 测试绿 ≠ 画对了：几何断言要有不依赖被测代码自我报告的观测通道（元素 rect / PNG 像素）。

## 5. 遇到问题

- **cargo 找不到 `vendor/rune`**：重跑 §1.3 子模块初始化。
- **`BUILDQ: workdir mismatch`**：bindings 里 requester 的 workdir 和你传的不一致，修正该行。
- **`BUILDQ: unknown requester`**：submit.sh 只有绑定校验（不强制登记）；若报此错，说明你用的是旧版 submit.sh，换成仓库版 `scripts/rune-buildq/submit.sh`。
- **worker 心跳停了**：`touch $RUNE_BUILDQ/STOP` 等退出 → `rm STOP` → 重跑 §1.5 的 nohup；遗留 job 会自动回收。
- **T 编号对不上**：源码变动会重排编号；状态按 React 文件集识别，不会丢。以 `--shard N` 实测为准。
- **跨段依赖**：segment 按 area 切分，同 area 上游在本段内；若确实需要别的段的未完成件，先做本段其它任务并在 push 时说明，由协调者在汇总时处理。
