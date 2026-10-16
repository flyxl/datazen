# B8 · platform/adapter.rs 867 行，超单文件 800 行推荐值

**状态**：开放 · **Wave-4 · w4-b8-adapter 轨修复中**

## 位置

`src-tauri/src/platform/adapter.rs`，基线 849 行，Wave-1 Track C 后 867 行。

## 实测订正（Wave-4 复核，推翻了「按职责拆生产代码」这个默认修法）

- `#[cfg(test)]` 起于 **`:646`** ⇒ **生产代码只有 1–645 = 645 行，本来就达标**；
  超出的 222 行**全部是测试模块**。所以正确修法是**把测试模块搬出去**，不是拆生产代码。
- 该文件同时是**被当文本扫描的 fixture**，多处 `include_str!` 把它整篇读进去做源码断言：
  - `adapter.rs:822` —— `crate::platform::production_source(include_str!("adapter.rs"))`
  - `adapter.rs:833` —— `let source = include_str!("adapter.rs");`，紧接着 :834 是 `source.split("#[cfg(test)]").next()`
  - `ipc.rs:307` —— `include_str!("adapter.rs")`，位于 :293-313 的模块清单里
  - `handles.rs:406` —— `("adapter.rs", include_str!("adapter.rs"))`，位于 :399-407 的清单里
- **落点订正**：本文原写的「`handles.rs:402`、`ipc.rs:311`」实际指向的是
  `include_str!("../../../packages/application/src/frontend_bridge.rs")`，**不是 adapter.rs**。

## 为什么当初没当场拆

拆它会牵动 `handles.rs` / `ipc.rs` 的 `include_str!` 扫描清单与 `bootstrap/app_state.rs`，
与各轨「最小冲突面」要求正面冲突。**明知超限而按下不表，是因为拆分本身就是一次独立变更。**

## 修法

在集成分支上作为单独变更处理（Wave-4 · `w4-b8-adapter`）：
把 `#[cfg(test)] mod tests`（:646-867）搬到独立文件，**只改路径与可见性**，
不得改断言、不得删测试、不得放宽断言；同步修正测试内 `include_str!("adapter.rs")` 的相对路径，
并确认两张模块清单仍然自洽（是否为穷尽清单、:834 那句 split 是否变成空操作）。

## 关闭条件

`src-tauri/src/platform/adapter.rs` ≤ 800 行，测试计数与搬迁前**相等**，
且 `pnpm test:platform-arch`（即 `node scripts/check-platform-crate-boundaries.mjs`）与 
`cargo test -p datazen --lib` 全绿。
