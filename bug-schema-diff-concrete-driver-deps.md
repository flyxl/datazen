# BUG · schema-diff 直接依赖具体 driver crate

**状态**：开放 · 从未裁定 · 非本次下沉引入

## 症状

`packages/schema-diff` 依赖 `datazen-driver-mysql` / `-postgres` / `-sqlite` 三个具体实现 crate，
而不是走 `datazen-driver-api` 的 trait。**违反分层契约** —— 中间层在编译期绑死三种具体方言。

## 影响

新增方言需要改 `schema-diff/Cargo.toml`；更换实现方言的代价被抬到编译期。
与 `shared-boundaries-and-ports.md` §2.1 的依赖方向相悖。

## 待裁定

先实测 `driver-api` 是否已提供这三种方言所需的一切能力（含 `SchemaMigrationRenderer` 的注册路径）。
能力齐备则改走 registry；若有缺口则先补契约。**不得凭推断动手。**

## 关闭条件

`packages/schema-diff/Cargo.toml` 中不再出现任何 `datazen-driver-{mysql,postgres,sqlite}`。
