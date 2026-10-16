仓里可留的证据已在此固化，探针工作树已删除，主 checkout 与集成分支状态均已复核未被污染。

### 独立验证 · 第 1 段（V1–V7 结构性核查）

验证方 `c2d54385`（全新实例，独立工作树 `.worktrees/verify-platform-infra`，起止指纹 `cd1ffcdee3983c902a20a92b…` 一致）。

| 门禁 | 期望 | 实测 | 结论 |
| --- | --- | --- | --- |
| V1 血缘 | 父=`179c6c8b6…`，count=1 | 父=`179c6c8b67022cbc8f86207c9e6c409b55525075`，count=`1` | PASS（未被 amend） |
| V2 改动面 | 恰 5 文件 | `wc -l`=`5`，无 `Cargo.lock`、无 codegen | PASS |
| V3 逐字下沉 | 仅注释差异、正则保留 | diff 仅 2 个注释 hunk，**零可执行代码变化**；4 个正则全为 `OnceLock`+`get_or_init`，`Regex::new` 仅出现在 4 处闭包内；3 个 pub fn、5 个单测齐全 | PASS |
| V4 薄壳形态 | 8 行、`pub use` | 8 行，`pub use datazen_platform_infra::redaction::{…}`，非 `pub(crate) use` | PASS |
| V5 宿主改动面 | 仅 `log_redact.rs` | `1 file changed, 6 insertions(+), 116 deletions(-)` | PASS |
| V6 调用点 | 真实命中数 | grep **19** 命中 = **18 真调用点** + 1 条文档注释（`log_redact.rs:4`）；调用点文件 diff 计数 `0` | PASS |
| V7 新 crate 依赖 | 无 `datazen-*` | 仅 `regex`、`thiserror` | PASS |

两个数字澄清落账：**真实调用点 18**（简报原写 15 有误，提交方 18 正确）；grep 第 19 条是本次新增的文档注释。
V3 的「`OnceLock` 惰性编译是否被改成每次 `Regex::new`」是最容易在纯搬运中悄悄退化的点，验证方逐处确认无回归。

#### D1 裁定：判为**非缺陷**，按原样合并

验证方报：`packages/platform-infra/src/redaction.rs:3-17` 新增注释把「`packages/ai-providers` 在
`protocol/mod.rs` 手写了第二份更弱的脱敏实现」当作**既成事实**，而在本轨树上实测该路径不存在：
`find packages/ai-providers -name '*.rs'` 仅 `1` 个（只有 `src/lib.rs`），整包脱敏相关命中 `0`，
`src/lib.rs:3` 自述「Wave-2 scaffold. 内容尚未迁入」。建议改写为对未来风险的表述。

**裁定：不修改。** 理由：

1. 本轨基线 `78038d02b` 按排序约束**本就不含 Wave-2 代码**，缺该文件是分支隔离的必然结果，不是注释失实。
2. 交付物是**合并树**。合并后 `packages/ai-providers/src/protocol/mod.rs` 必然存在（Wave-2 已合入集成分支），
注释三条论断在发布的树里**逐条为真**——协调者已在 `d268a7ded` 逐行核对过：
函数确实在 `protocol/mod.rs:85-104`；确实只做 `split_once('?')` 与 `split_once('@')`；
函数体内无任何 regex，确实不处理 `Bearer` 与 `"password":`。
3. 这段注释在合并树里的**价值恰恰高于假设式表述**：它记录了那份重复实现的来由、它的退化之处，
以及它必须在 Wave-3 被删除的理由——这是台账之外的第二处长期留痕。

按项目纪律，验证方必须在**它所处的树**上取证，它做的没错；错的是把分支基线造成的差异读成缺陷。
这条本身是台账级的教训：**跨轨分支上的「事实性错误」，必须先问「合并后是否仍然如此」再判缺陷**。

#### 两项记账（非本轨缺陷，不阻塞）

- `packages/platform-infra/Cargo.toml` 的 `thiserror = "1"` 在本 crate 内零使用，由预置提交 `179c6c8b6` 带入，本轨未动。属依赖卫生问题，留待 infra 层后续 crate 真正用到或直接删。
- `progress.md`（64 行）随轨入库合规，但**合并收尾时必须删除**，否则违反「交付即销毁」台账纪律。
