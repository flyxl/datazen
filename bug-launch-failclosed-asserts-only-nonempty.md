# B18：`launch` 的 fail-closed 断言只校验「非空主体」，占位主体可以溜过

- **状态**：开放 · 已确认（实测；属**既有测试强度缺口**，**非本波引入**）
- **归属**：`src-tauri/src/platform/identity.rs` 与被测的 `adapter_tests.rs` 在本波**均未被功能性改动**
  （identity.rs 的 sha 与基线相同），同一变异在基线上同样全绿 ⇒ **不是回归**。
  由 Wave-4 · w4-b8-adapter 的独立验证方（ed5f3dd0）在验证「测试是否还有判别力」时顺带发现；
  验证方已明确按「回归归因」口径判为非阻断，并请协调者裁定是否另立 —— 现另立。

## 实测

`src-tauri/src/platform/identity.rs` 的文档明写「绝不伪造占位主体」，而
`platform::adapter::tests::launch_without_a_deterministic_desktop_user_fails_closed` 只断言主体**非空**：

```rust
assert!(!adapter.identity().principal_id().as_str().is_empty());
```

验证方的边界探针：把 `identity.rs` 的 fallback 换成 `PrincipalId::new("placeholder-unknown")`（一个**非空占位**主体），
在 `env -u USER -u DATAZEN_DESKTOP_USER` 下跑 `cargo test -p datazen --lib -- platform::adapter::tests`
⇒ **13 passed / 0 failed**，全绿。

⇒ 一个伪造的占位主体可以完整通过这条 fail-closed 断言，而**断言名与 identity.rs 的文档都声称它守住了这件事**。

## 影响

- 「无确定性桌面用户时必须失败关闭」这条不变量**实际没有被守住** —— 守住的只是「主体非空」。
- 与 B5 同一失效模式：**断言名 / 文档声称的强度 > 实际断言强度**。
- 这类缺口的共同危害是：它让「已覆盖」的记录失真，而门禁全绿，所以能长期存活。

## 修法

把断言从「非空」升级为「**不是占位值**」，两条路任选：
1. 断言该调用**返回错误** —— fail closed 的本义是**拒绝**，而不是给一个空/占位主体；
2. 或断言主体不属于任何已知占位集合。

并补一条**负控**：注入非空占位主体时该断言必须变红。

## 关闭条件

注入一个非空占位主体后，`launch_without_a_deterministic_desktop_user_fails_closed`
（或其后继断言）**变红**；且给出该负控的逐字结果。
