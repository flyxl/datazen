# 并行开发协调目录

本目录存放**跨 worktree 共享**的协调 artifacts。主检出（`datazen/`）为权威源；各轨道 worktree 通过 bootstrap 脚本将 `hub.md` 软链到此处，实现实时进度合并而不产生 git 合并冲突。

## 文件布局

```text
coordination/
├── README.md                          # 本说明
├── hub.md                             # 公共进度总览（协调者维护，各轨只读或按规则追加）
├── code-review-remediation-plan.md    # 本次代码审查 remediation 实施计划
└── tracks/
    └── <track-id>/
        ├── progress.md                # 该轨功能进度（编码/测试代理写本轨小节）
        └── bugs.md                    # 该轨独立 bug 清单（禁止写入其他轨 bug 文件）
```

## 纪律

1. **Bug 清单按轨隔离**：每个 track 只读写 `tracks/<track-id>/bugs.md`，禁止把所有 bug 堆进单一文件。
2. **Hub 由协调者融合**：各轨完成 milestone 后，协调者将摘要合并进 `hub.md`；代理不得直接改 hub 总览表（除非简报明确授权）。
3. **Hub 软链**：worktree 内 `docs/development/coordination/hub.md` → 主检出同路径；编辑 hub 即全局可见。
4. **Git 提交**：`tracks/<track-id>/*` 随各轨分支 commit；`hub.md` 与 `plan.md` 在合并里程碑时由协调者在集成分支 commit。
