# BUG · platform-infra 声明了零使用的 thiserror

**状态**：开放 · 低 · 由预置提交 `179c6c8b6` 带入，**非 C3 轨引入**

## 位置

`packages/platform-infra/Cargo.toml:12` `thiserror = "1"`，crate 内**零处使用**。

## 修法

二选一，都可：删掉；或留给 infra 层后续真正需要的错误类型
（C3 轨刻意未新增错误类型，因为脱敏函数不返回 `Result`）。

## 关闭条件

`packages/platform-infra/src/` 中 `thiserror` 有命中，或该依赖行已删。
