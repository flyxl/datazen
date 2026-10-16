# BUG · OwnerRef 契约重复定义

**状态**：开放 · Wave-4 · w4-ownerref 轨修复中

## 位置

实测 `grep -rn 'enum OwnerRef' src-tauri/ packages/` 只有**两处定义**
（Wave-4 · w4-ownerref 轨复核）：

- `packages/platform-api/src/context.rs:65` —— 契约侧定义
- `packages/runtime/src/connection/types.rs:381` —— **共享内核里的第二份定义**

**订正**：本文件原先把宿主侧落点写成 `src-tauri/src/platform/handles.rs:192`，
那是 `OwnerRef::Job` 的**使用点**（变体匹配），不是定义处。
该文件是否 import / 再导出此类型，由修复轨自行核实。

## 说明

Wave-1 只记录不改：它属于「契约归属」问题而非迁移问题，在下沉轨里动会扩大冲突面。

## 修法（待裁定）

宿主侧删除自有定义，全量改用 `platform-api::context::OwnerRef`。
**前提**：先核实两者的变体集合是否完全一致。若宿主有 platform-api 没有的变体，
则先补契约再改宿主，顺序不能反。

## 关闭条件

`grep -rn 'enum OwnerRef' src-tauri/ packages/` 只剩 `platform-api/src/context.rs` 一处。
