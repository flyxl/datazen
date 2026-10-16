# BUG · platform/adapter.rs 867 行，超单文件 800 行推荐值

**状态**：开放 · 需专门变更 · 不得在任何下沉轨顺手做

## 位置

`src-tauri/src/platform/adapter.rs`，基线 849 行，Wave-1 Track C 后 867 行。

## 为什么没当场拆

拆它会牵动 `handles.rs` / `ipc.rs` 的 `include_str!` 扫描白名单与 `bootstrap/app_state.rs`，
与各轨「最小冲突面」要求正面冲突。**明知超限而按下不表，是因为拆分本身就是一次独立变更。**

## 修法

在集成分支上作为单独变更处理，按职责拆分（建议：会话生命周期 / 句柄管理 / 上下文组装）。
拆分前先确认 `handles.rs:402`、`ipc.rs:311` 两处 `include_str!` 的新路径。

## 关闭条件

该文件 ≤ 800 行，且 `pnpm test:platform-arch` 与 `cargo test -p datazen --lib` 全绿。
