# Track S5-B: BindParamPanel Upgrade & Parameter History

## Status: READY_FOR_TEST

## Commit
`7f01adb31` — feat(s5-b): BindParamPanel upgrade & parameter history

## Summary
Completed all S5-B steps: 5-syntax bind param parsing, BindParamPanel upgrade
with legacy + stableId modes, history dropdown, and useBindParameters hook.

## Changed Files
| File | Lines | Description |
|------|-------|-------------|
| `src/lib/sqlBindParams.ts` | +39 | Five-syntax parsing, stable IDs, v2 wire payload, labels, fingerprint, sensitive filtering |
| `src/components/query/BindParamPanel.tsx` | +283/-18 | Dual-mode panel (legacy/stableId), history dropdown, keyboard nav, a11y |
| `src/components/query/__tests__/BindParamPanel.test.tsx` | +248 | 14 tests: empty, legacy mode, stableId mode, active highlighting, history UI |
| `src/lib/__tests__/sqlBindParams.test.ts` | +102 | 36 tests: fixtures, five syntaxes, shared names, ordinals, sensitive, fingerprint |
| `src/windows/connection/query/useBindParameters.ts` | +221 | New hook: value reconciliation, history persistence, payload builder |
| `src/windows/connection/query/__tests__/useBindParameters.test.ts` | +247 | 24 tests: params, setValue, reconcile, payload, history, localStorage degradation |

## Test Results
- `npx tsc --noEmit`: **0 errors**
- `npx vitest run`: **3174 passed, 2 failed** (pre-existing docs-consistency only)
- S5-B tests: **74/74 pass** (36 + 14 + 24)

## Notes
- No modifications to QueryPanel, execution gate, or Rust binder
- No `pnpm install` run
- `useBindParameters.ts` hook output ready for S5-C consumption
- Sensitive param names (password/token/secret/key/credential) excluded from history
- History stored in localStorage with silent degradation on all failure modes
