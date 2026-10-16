# BUG · OwnerRef 契约重复定义

**状态**：开放 · 待集成分支裁定

## 位置

- `src-tauri/src/platform/handles.rs:192` —— `OwnerRef::Job`
- `packages/platform-api/src/context.rs` —— `OwnerRef`

## 说明

Wave-1 只记录不改：它属于「契约归属」问题而非迁移问题，在下沉轨里动会扩大冲突面。

## 修法（待裁定）

宿主侧删除自有定义，全量改用 `platform-api::context::OwnerRef`。
**前提**：先核实两者的变体集合是否完全一致。若宿主有 platform-api 没有的变体，
则先补契约再改宿主，顺序不能反。

## 关闭条件

`grep -rn 'enum OwnerRef' src-tauri/ packages/` 只剩 `platform-api/src/context.rs` 一处。
