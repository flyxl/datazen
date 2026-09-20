# Bugs — ai-frontend Track

## ai-frontend-BUG-001

- **Status**: 已修复
- **Severity**: Medium
- **Description**: Chat persistence session key hardcoded to `default::` — localStorage persistence does not respect session isolation.
- **Reproduction Steps**:
  1. Connect to database A, send 3 chat messages.
  2. Switch to database B, send 3 chat messages.
  3. Reload the page.
  4. Re-open both chat sessions — both will load the combined history from `default::`.
- **Root Cause**: In `src/stores/aiStore.ts` line 701, `handleStreamChunk` persists chat history using `computeSessionKey(undefined, undefined, undefined)` which always produces the key `'default::'`. The session key for persistence should match the key used during `initChatSession`.
- **Impact**: Cross-connection chat session isolation works in-memory (during a single page session) but does NOT persist across page reloads. All connections share a single persisted chat history.
- **Fix**: Store the `sessionKey` on the `chatSession` object (set during `initChatSession`) and use it during persistence in `handleStreamChunk`. Fixed in commit `a2a36bca6`.

## ai-frontend-BUG-002

- **Status**: 已修复
- **Severity**: Medium
- **Description**: `AiChatPanel` never passes `dbSessionId`/`database` to `initChatSession()`, making the session key parameters in the store function unreachable from the UI.
- **Reproduction Steps**:
  1. Open a chat panel with a specific `dbSessionId` and `database` prop.
  2. Inspect the `initChatSession` call — it's called as `initChat()` with no arguments.
- **Root Cause**: In `src/components/ai/AiChatPanel.tsx` line 97, `initChat()` is called without passing the component's `dbSessionId` or `database` props. Even though the store's `initChatSession` accepts these parameters, they are never provided by the UI.
- **Impact**: Combined with BUG-001, this ensures session isolation for persistence is completely non-functional. Even if BUG-001 were fixed, this bug would still cause all sessions to use the default key.
- **Fix**: Pass `dbSessionId` and `database` to `initChat(dbSessionId, database)` in the `useEffect` at line 96-98. Fixed in commit `a2a36bca6`.

## ai-frontend-BUG-003

- **Status**: 已修复
- **Severity**: Low
- **Description**: Type declaration mismatch — `initChatSession` type in `types.ts` declares `() => void` but implementation accepts `(connectionId?: string, dbSessionId?: string, database?: string)`.
- **File**: `src/stores/ai/types.ts` line 119 vs `src/stores/aiStore.ts` line 403.
- **Impact**: While TypeScript allows this due to parameter bivariance, the type hides the fact that callers should pass connection context. This contributed to BUG-002 being introduced.
- **Fix**: Update the type to `initChatSession: (dbSessionId?: string, database?: string) => void`. Fixed in commit `a2a36bca6`.
