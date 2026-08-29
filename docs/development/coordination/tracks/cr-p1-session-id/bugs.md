# cr-p1-session-id — Bug 清单

| Bug ID | 描述 | 状态 | 记录时间 | 重现步骤 | 验证记录 |
|--------|------|------|----------|----------|----------|
| BUG-001 | `extensionBridge` 单测未随 live-session 解析更新，6 例失败 | 已修复 | 2026-08-29 | `npx vitest run src/lib/__tests__/extensionBridge.test.ts src/lib/__tests__/extensionBridge.security.test.ts`；未 seed `activeConnectionStore` 的 `command.invoke` 用例收到 `E_NOT_FOUND` 而非继续执行 | 修复代理：seed `activeConnectionStore` + 断言 `live-*`；vitest 49/49 |
| BUG-002 | `cargo test -p datazen --lib` 全量 1 失败：`connect_dedicated_opens_separate_session_from_reuse` | 已修复 | 2026-08-29 | `CARGO_TARGET_DIR=.../target cargo test -p datazen --lib`；MockDriver `connect` 固定返回 `mock-{config.id}`，`connect` 与 `connect_dedicated` 得到相同 id | 修复代理：MockDriver 递增 session id + reconnect 保留 db_session_id；cargo 1174/1174 |

## BUG-001 详情

**影响：** 390d4efc 将 `extensionBridge.handleCommandInvoke` 改为从 `activeConnectionStore` 解析 live `dbSessionId`，无活动会话返回 `E_NOT_FOUND`。`extensionBridge.test.ts` / `extensionBridge.security.test.ts` 中仍用 `connectionId: 'c'` 且期望 `dbSessionId: 'c'`，或未 seed store。

**失败用例：**

- `extensionBridge.test.ts`: rate limit / timeout（20 并发 `command.invoke` 在 session 检查阶段即失败）
- `extensionBridge.security.test.ts`: INTERNAL 错误码期望、`args:null` / array args / prototype pollution 转发

**修复方向：** 在相关 `beforeEach` 或各用例 seed `activeConnectionStoreState.connections[c] = { status: 'connected', dbSessionId: 'live-c', ... }`；断言改为 `dbSessionId: 'live-c'`。

## BUG-002 详情

**影响：** progress 要求 `cargo test -p datazen --lib` 全绿；当前 1173 passed / 1 failed。

**断言：** `connect_dedicated_impl` 与 `connect_impl` 对同一 `connection_id` 应返回不同 `db_session_id`；MockDriver 每次 `connect` 返回 `mock-{config.id}`，导致 dedicated 覆盖同 key 会话且 id 相同。

**修复方向：** MockDriver 生成唯一 session id（如递增 / UUID），或 dedicated 路径在 mock 测试中显式断言 ref-count 语义而非 id 不等。
