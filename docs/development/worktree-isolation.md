# Worktree 隔离边界

> 本文只回答一件事：**多个子代理各自在 worktree 里开发，为什么合并阶段仍会互相影响。**
> 本文只讲 git 与 worktree 的实测行为，不含任何轨道台账或进度约定。

## 1. 隔离的是工作树，不是仓库

`git worktree add` 为每个工作目录生成一个**私有 gitdir**，但它通过 `commondir` 指回根 `.git`：

```text
主检出 .git/
├── objects/            ← 共享：所有 commit / blob
├── refs/heads/         ← 共享：所有分支，只有一份
└── worktrees/
    └── datazen-ui-extract/
        ├── commondir   ← 私有，内容是 "../../"，指回根 .git
        ├── HEAD        ← 私有：ref: refs/heads/feature/ui-extract
        ├── index       ← 私有：暂存区
        └── logs        ← 私有：仅本 worktree 的 HEAD reflog
```

| 私有 | 共享 |
|------|------|
| 工作目录文件、暂存区、`HEAD`、HEAD reflog | 对象库、`refs/heads/*` 全部分支、**分支 reflog** |

因此「在不同 worktree 工作」只保证**文件互不可见**，不保证**引用互不影响**。同一个分支名在 14 个 worktree 里解析出的是同一个 sha——谁移动它，其余全部立刻看到。

自查一行：

```bash
cat "$(git rev-parse --git-dir)/commondir"   # 输出 ../.. 即确认共享
```

## 2. 由此产生的四类干扰

### 2.1 引用被间接移动

分支 reflog 是共享的，所以**任何人对该分支的移动都会留痕**。定位手法：

```bash
git reflog show feature/<track> | head        # 谁、何时、动了它
```

实例：`feature/ui-extract` 被重建到 `9c8a2808a` 后，又被一次 `reset` 移回旧值 `57bcf41cd`；reflog 留下 `reset: moving to 57bcf41cd`。若发现分支位置与预期不符，**先查 reflog 再下结论**，不要直接重建——重建会覆盖现场。

### 2.2 一个分支只能被一个 worktree 检出

这是 git 对「两个代理抢同一分支」的**唯一硬拦截**。移动一个正被检出的分支会失败：

```text
fatal: 无法强制更新检出于 '.../datazen-ui-extract' 的分支 'feature/ui-extract'
```

遇到这个报错，正确做法是**在持有该 worktree 内改**（`git -C <worktree> reset`），不要绕道 `--force` 或改写 `packed-refs`。

### 2.3 worktree 名册是全局的

`git worktree list` / `add` / `remove` 影响所有人。**为验证合并安全性而建的临时 worktree（如 `/tmp/*`）同样会出现在别人的名册里**；用完必须清理，否则留下指向主仓库的僵尸登记。Coder 红线已禁止删除 worktree，但**协调者自己建的临时 worktree 同样要收尾**。

### 2.4 rebase 基准会移动

并行合入时，集成分支尖端在 rebase 期间可能已被别人推进。因此：

- **合入前实测，不要靠算术**。两点差值会因基准移动而失真（见 §3）。
- 轨道从远端基底切出、落后很多时，`git diff A..B` 统计的是两个**尖端**的差，不是本轨自己的改动；应以 merge-base 为基准分别测量两侧。

## 3. 两点差值算不出删除

实例：某轨从旧基底切出并带着一条与本轨无关的文档清理。rebase 到集成分支后，`merge-base` **恰好等于集成分支尖端**（rebase 把它变成了自己的基底），两点差值只显示 231 文件，**不会告诉你其中是删除**。真实情况是净删 169 个文件，含 93 份开发文档、5 份架构 RFC、2 个 CI 脚本。

只有在一个一次性 worktree 里真跑一次 `git merge` 才量得出来：

```bash
git worktree add --detach /tmp/<name> <integration-tip>
git -C /tmp/<name> merge --no-commit --no-ff <track>
git -C /tmp/<name> diff --cached --name-status | grep -c '^D'   # 期望的删除数
git worktree remove --force /tmp/<name>
```

**这是本手册里唯一必须实跑而不能算的检查。**

## 4. 测试结果同样跨 worktree 不可比

同一提交在不同 worktree 里跑出的失败数可能不同，与代码无关：

- **`packages/pro-extensions/sql-editor-pro` 是独立 git 仓库且被 gitignore**，新 worktree 里没有它，约 10 个测试文件必然失败。**基线必须在同一 Pro 状态下取。** 跨 Pro 状态比较失败集合是范畴错误。
- 在 Pro 缺失的 worktree 里量基线，要 `git checkout --detach <集成尖端>` **在该 worktree 内部**跑，再切回；不要去另一个 worktree 取数字。
- `node_modules` 是逐 worktree 的实体目录。**用符号链接指向主检出的 worktree，其测试结果不可信**——模块解析会失败，而失败清单读起来像代码回归。

## 5. 陈旧 .driver-file-stash 会静默回滚合并后的托管清单文件

`scripts/resolve-drivers.mjs` 注入驱动依赖前，会先把三个托管清单文件（`Cargo.toml`、`src-tauri/Cargo.toml`、`Cargo.lock`）的干净副本存进 `.driver-file-stash/`，构建后再 `--restore` 还原。注入时读的基线是 `managedReadPath()`：**只要 stash 副本存在，就以它为准重写工作区文件**。

`.driver-file-stash/` 是 gitignored 的，`git checkout -- .`、`git reset --hard` 都清不掉它。旧实现把「本次会话外层调用者刚建好的 stash」和「上一次会话遗留的 stash」当成同一件事——`allStashed()` 为真就跳过重建。于是这个顺序会静默吃掉别人的合并：

1. 会话 A 在合并之前跑过一次 codegen（`--drivers=all`），留下 stash；
2. 把别的分支合并进来，`src-tauri/Cargo.toml` 的 `[dev-dependencies]` 拿到了新内容；
3. 会话 B 再跑 codegen：跳过重建 stash，以合并前的旧副本为基线重写 `src-tauri/Cargo.toml`，**退出码 0，不报任何错**。
   被吞掉的是真正被注入的那三个标记段所在的文件。当前 15 个驱动**全是 path 驱动**，
   根 `Cargo.toml` 不生成 `[patch]`（实测其 diff 为空），所以可见的破坏发生在 `src-tauri/Cargo.toml`；
   一旦将来引入 git 驱动，根 `Cargo.toml` 同样会被重写。

结果是占位段被填好了、工作区看起来正常，但合并进来的改动被回滚了；`.driver-file-stash/` 不受 git 管辖，任何一次 `git checkout -- .` 都不会把它暴露出来。

### 复用前判新鲜度，陈旧即硬失败

- 建 stash 时写一份旁证 `.driver-file-stash/.stash-meta.json`，记录当时的 `head` 与 `createdAt`；`restore` 走 `cleanupStashDir()` 整目录删除，旁证随之消失，不会留下「有元数据没 stash」的僵尸状态。**旁证本身不在 `MANAGED_FILES` 里**——它描述的是这次 stash，不是被托管的清单文件。
- 复用前调 `evaluateStashFreshness()`。它是纯函数，只吃 `{ stashHead, currentHead, managedFilesChangedBetween }`：

| stash HEAD | 当前 HEAD | 托管文件 | 判定 | 理由 |
| --- | --- | --- | --- | --- |
| 相同 | 相同 | — | 复用 `same-head` | 同一提交，不可能有东西在基线下面变过 |
| 不同 | 不同 | 未变 | 复用 `managed-files-unchanged` | 变的是别的文件，与注入基线无关 |
| 不同 | 不同 | 有变 | **硬失败** `managed-files-changed` | 基线已过期，复用等于回滚合并 |
| 缺失 | — | — | **硬失败** `missing-metadata` | 年龄无法证明，不猜 |
| — | 读不到 | — | **硬失败** `unknown-current-head` | 同上 |

- 陈旧时抛 `StaleDriverFileStashError`，CLI 退出码 1，并打印可直接照抄的恢复命令。抛出点在 `main()` 的 `try` 之外，**因此不会触发 main-catch 里的自动 `restoreManagedFiles()`**——自动 restore 正是把合并覆盖掉的那一步。
- `--restore` 不加同样的闸：它在已知坏掉的现场上是运维唯一能主动执行的还原动作，硬阻断只会把人堵死。
- `node scripts/driver-file-stash.mjs status` 只读地打印 `freshness: STALE|fresh (reason=...)`、两个 HEAD 与变化的文件清单。

### 恢复步骤

报错信息里带的就是这几行，照抄执行：

```bash
git diff -- Cargo.toml src-tauri/Cargo.toml Cargo.lock > /tmp/managed-files.patch
rm -rf .driver-file-stash
git checkout HEAD -- Cargo.toml src-tauri/Cargo.toml Cargo.lock
node scripts/resolve-drivers.mjs <相同参数>
```

**此时不要跑 `node scripts/driver-file-stash.mjs restore`（或 `--restore`）**：它把陈旧 stash 的内容拷回工作区，用合并前的旧副本覆盖当前合并结果——那正是本节要防的故障。

### 顺序坑

新 worktree 里「先合并、后跑 codegen」本身是安全的；真正要避免的是**合并之后还留着合并之前建的 stash**。现在的闸门把这种情况变成一次显式的非零退出加一行恢复命令，而不是继续静默吃掉合并。

## 6. 红线补充

在下列红线之外：

- 禁止 `git branch -f`、`git update-ref`、`git worktree remove`、`git gc`——它们作用于共享状态。
- 禁止在 worktree 内 `git checkout` **他人分支**；需要别人的产物时读对方的 worktree 绝对路径。
- 临时 worktree 必须在同一会话内清理，并在报告中列出清理命令。
- 发现分支位置异常时，**先 `git reflog` 再动作**；reflog 是唯一现场，重建会毁掉它。


## 7. 新建 worktree 的完整初始化

`git worktree add` 只建立检出，**不产生任何生成物**。跳过下面三步的 worktree 会在第一次
`cargo` / `cargo fmt` 时以一条与真实故障无关的报错失败：

```text
error[E0463]: can't find crate for `std`
error: failed to resolve: use of undeclared crate or module `driver_init`
```

「找不到 std」不是工具链坏了，是缺 `src-tauri/src/driver_init.rs`（codegen 产物，见 .gitignore）。
三条命令**顺序不可换**：`node_modules` 的模块解析要先成立，`resolve-drivers.mjs` 才有东西可写。

```bash
MAIN=/path/to/main-checkout          # 主 checkout：只读，不写
WT="$MAIN/.worktrees/<name>"; SHA=<commit>; BR=<branch>

cd "$MAIN" && git rev-parse --short HEAD          # 建树前记录主 checkout 位置
git -C "$MAIN" worktree add -b "$BR" "$WT" "$SHA"   # 提交方
#                                                    验证方改用 --detach "$SHA"
cd "$WT"
ln -sfn "$MAIN/node_modules" "$WT/node_modules"     # ① 必须在下面两个生成脚本之前
node scripts/generate-builtin-locales.mjs          # ②
node scripts/resolve-drivers.mjs               # ③ 裸跑即 basic，不需要环境变量

# 驱动指纹 = 跨 worktree 门禁可比性的凭证
shasum -a 256 src-tauri/src/driver_init.rs | cut -c1-16
grep -c "^extern crate datazen_driver_" src-tauri/src/driver_init.rs

cd "$MAIN" && git rev-parse --short HEAD          # 建树后复核主 checkout 未被动过
```

### 驱动选型：默认 `basic`，只有验证 DB 注册表相关行为时才用 `all`

`src-tauri/src/driver_init.rs` 是 codegen 产物，内容随驱动选型而变（实测）：

| 调用方式 | 驱动数 | 文件大小 |
| --- | --- | --- |
`node scripts/resolve-drivers.mjs`（裸跑） | 4 | 807 B |
`DATAZEN_DRIVERS=all node scripts/resolve-drivers.mjs` | 15 | 1625 B |

**裸跑就是 `basic`，不需要任何环境变量。** 出处三处：

- `scripts/resolve-drivers.mjs:152-154` —— 无 `--drivers` 且无 `DATAZEN_DRIVERS` 时落到 `basic`（原话 `// Default: basic (four core path drivers).`）；
- `scripts/new-feature-worktree.sh:16` / `:203` —— 建树脚本第 1 层 codegen 固定 `--drivers=${NEW_WT_DRIVERS:-basic}`；
- `pnpm tauri:dev` —— 同样走 basic。

所以上面第 ③ 步原样可用。

跨 worktree 比对测试结果时，是不是该统一用 `all` 才可比？**不需要。** 实测同一提交 `bc36cd121` 下，
basic（4 驱动 / 807 B）与 all（15 驱动 / 1625 B）各跑一次 `cargo test -p datazen --lib`，结论行**逐字相同**，退出码都是 0：

```
basic: test result: ok. 1552 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out
all  : test result: ok. 1552 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out
```

驱动选型改变的是 `driver_init.rs` 与宿主的可选依赖，**不改变这套单测的用例集合**。

**这条实测的适用范围必须说清**：上面两个数字证明的只是**宿主 Rust 单测**对选型不敏感。
驱动选型确实会改变前端可见的数据库清单 —— 建树脚本自己在 `scripts/new-feature-worktree.sh:516` 写着：

> 未设置 `NEW_WT_DRIVERS` ⇒ 第 1 层 codegen 固定 `--drivers=basic`（postgres / mysql / sqlite / redis）；
> 主检出与 CI 通常跑 `--drivers=all`（全部 path 驱动，多出 sqlserver / mongodb / clickhouse 等）。**两者不是同一个 DB_REGISTRY**。

所以：**凡是要验证与 `DB_REGISTRY` / 驱动清单元数据相关的行为（前端 `generated.ts`、`src/extensions/*`、驱动 UI 元数据），
必须显式选 `all`（`NEW_WT_DRIVERS=all` 建树，或 `DATAZEN_DRIVERS=all node scripts/resolve-drivers.mjs`），假条若用默认 basic，前端可见的数据库清单就是短的。**
纯 Rust 逻辑 / 契约 / 门禁脚本类的轨用默认 basic 即可。

前端全套同样不敏感（实测）：同一提交 `acc0f1667` 下，basic 树（4 驱动 / 807 B）跑 `npx vitest run` 得
`Test Files 602 passed (602)` / `Tests 6335 passed (6335)`，与 all 选型下 R 阶段的记录**逐字相同**，退出码 0。

**结论**：宿主 Rust 单测与前端全套对驱动选型都不敏感；受选型影响的只有 DB 注册表本身。

真正的不变量只有一条：**参与同一次比对的树必须用同一种驱动选型**。不满足时，「宿主少了几个测试」
这类结论才真的毫无意义（见 §4）。判断靠**现算指纹**，别照抄本文或任何台账里的值：

```
bash
shasum -a 256 src-tauri/src/driver_init.rs | cut -c1-16
grep -c "^extern crate datazen_driver_" src-tauri/src/driver_init.rs
```

### 顺序坑

- **不要用 `set -e` 包住这三步**：生成脚本失败时脚本先死，`EXIT=$?` 永远打不出来，
  而门禁退出码必须能被拿到。
- **`resolve-drivers.mjs` 会按设计弄脏 `Cargo.toml`、`src-tauri/Cargo.toml`、`Cargo.lock`**（第 5 节那三个托管清单文件）。
  要往这棵树上合并的分支，在 `git merge` 前必须 `git checkout --` 复位，否则合并会被拒。
- **worktree 里第一次跑 `pnpm` 脚本可能会真的装一次**（`Packages: +N -M` 然后继续）。那不是
  「符号链接造成的噪声」，而是 pnpm 在干正事：合并进来的分支若动过 `package.json` / 锁文件，
  共享的 `node_modules` 就落后了，它必须补齐一次。**不要用 `verifyDepsBeforeRun=false` 关掉它**
  ——（1）那会把「依赖真的过期了」这个信号一起关掉；（2）实测 pnpm 11 的 `.npmrc` 里这个键
  不生效（`pnpm config get verifyDepsBeforeRun` 返回 `undefined`），写进 `pnpm-workspace.yaml`
  则是**全仓**生效、连 CI 一起关。装完一次之后第二次跑就不再装了，这才是正常状态。
- **不要用 `pnpm install` 代替 ①**。`node_modules` 是符号链接时裸 `pnpm run` 会以退出码 1 失败
  （依赖校验认不出链接目标），要写成 `pnpm --config.verify-deps-before-run=false <script>`。
- **`node scripts/platform-arch-selfcheck.mjs` 在脏树上直接退出 2。** 它在运行首尾各取一次 `git status --porcelain` 做自证
  （`scripts/platform-arch-selfcheck.mjs:538` / `:576`），树一脏就不跑。而 `resolve-drivers` 按设计会把托管清单弄脏，
  ⇒ **在刚建好的 worktree 里直接跑自检必然失败**。先 `git checkout -- Cargo.lock src-tauri/Cargo.toml` 复位再跑。
  注意 `pnpm test:platform-arch` **不含**这个自检（它只是 `node scripts/check-platform-crate-boundaries.mjs`，见 `package.json:110`），
  所以这条只影响直接调用自检的人 —— 门禁链本身不受影响（两种状态下门禁结论行逐字相同，只有 advisory 数在 2/3 间浮动）。
- **删验证树前先 `unlink node_modules` 符号链接。** worktree 里的 `node_modules` 是指向主 checkout 的符号链接；
  `git worktree remove --force` 与 `rm -rf` 处理符号链接本身通常是安全的，显式先 `unlink` 能彻底排除
  「透过链接删掉主仓依赖」的可能。删完用 `git worktree list` 复核，并确认主仓 `git status` 仍为空。
