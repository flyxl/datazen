# B16：门禁脚本经符号链接路径调用时静默退出 0 且零输出（会被误读为 PASS）

- **状态**：开放 · 已确认（实测；Wave-4 · w4-panic-gate 的独立验证方 834a9c7d 发现，协调者采信）
- **归属**：既有仓库约定债。`scripts/check-platform-crate-boundaries.mjs:733` 用同一写法，
  另有 6 个脚本已改用 `pathToFileURL(resolve(...))`，说明仓库内部已知此坑但只改了一部分。**非本波引入。**

## 实测

脚本末尾的 CLI 入口判据形如：

```js
if (import.meta.url === 'file://' + process.argv[1]) { /* CLI 主体 */ }
```

当脚本经**符号链接路径**被调用时（worktree 路径、/tmp 下的链接、某些 CI 缓存路径都可能如此），
`import.meta.url` 是解析后的真实路径，而 `process.argv[1]` 是调用者给出的链接路径，两者不等
⇒ **CLI 主体整段不执行**，进程**退出码 0、标准输出 0 字节**。

实测：`node /tmp/dz-verify-panic/scripts/check-shared-crate-panic-policy.mjs` 与经 /tmp 额外符号链接调用，
均静默 EXIT=0、零输出。

## 危害

对门禁脚本来说这是最坏的失败形态 —— **它看起来就是「通过」**。
任何以符号链接方式调用门禁的包装脚本、或跑在 worktree 路径下的调用，都可能拿到「假通过」。
`scripts/check-platform-crate-boundaries.mjs` 同样受影响（既有问题）。

## 修法

入口判据改为不依赖字符串恒等：

```js
import { pathToFileURL } from 'node:url';
if (import.meta.url === pathToFileURL(process.argv[1]).href) { /* CLI 主体 */ }
```

（仓库已有 6 个脚本采用该写法，可照抄。）修完对每个受影响的脚本做一次「经符号链接调用」的实测，
确认 CLI 主体确实执行、输出非空。

## 关闭条件

全仓不再有以 `import.meta.url === 'file://' + process.argv[1]` 形式判入口的门禁脚本；
或所有门禁脚本统一改用具名 main 导出 + 显式入口，使「没执行」不可能静默退 0。
关闭前必须对每个受影响脚本给出「经符号链接调用输出非空」的实测证据。
