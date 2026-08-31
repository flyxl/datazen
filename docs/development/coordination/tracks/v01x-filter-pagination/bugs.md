# v01x-filter-pagination Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| 无 | 本轨定向 parser、组件 loading/error 和 pagination reset 测试未发现功能 bug | — | — | 56/56 个定向测试通过；diff check 通过；tsc 仅受基线生成文件阻塞 |

## 环境记录

- worktree 缺少 gitignore 的 `src/locales/builtinLocales.ts`；该目录由沙箱拒绝写入。组件测试通过临时 resolver 使用外部只读 stub 验证，未修改 locale/codegen，也未将环境问题登记为业务 bug。
- 本轨无 Rust 改动；Host 桌面 E2E 按约定留待 R。
