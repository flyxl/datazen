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
