# Track S4-A: Statement Frame, Gutter & Execution State

> Status: **READY_FOR_TEST**
> Branch: `feature/sql-s4-a`
> Base: `feat/sql-editor` (S1-S3 tracks merged)
> LastHeartbeat: 2025-01-01T00:00:00Z

## Phase

Coder → READY_FOR_TEST (completed)

## Deliverables

### New files (src/components/sql-editor/extensions/)

| File | Lines | Description |
|------|-------|-------------|
| `types.ts` | 72 | Shared types: `ExecutionStatus`, `EditorExecutionState`, `isDocumentDegraded()`, `shouldHideFrame()`, platform detection |
| `executionState.ts` | 93 | `StateField` + effects: `StartExecutionEffect`, `CancelExecutionEffect`, `FinishExecutionEffect`, `matchesRunningTarget()` |
| `statementFrame.ts` | 183 | Frame decoration `ViewPlugin` + `frameDegradedField` + `SetDegradedEffect` + theme tokens |
| `statementGutter.ts` | 297 | Gutter `GutterMarker` (PlayMarker) + `docVersionField` + click handler + platform-aware tooltip |

### Test files (src/components/sql-editor/extensions/__tests__/)

| File | Lines | Tests |
|------|-------|-------|
| `executionState.test.ts` | 209 | 16 tests — initial state, effects, matchesRunningTarget, isIdleOrCancelled, immutability |
| `statementFrame.test.ts` | 196 | 12 tests — isDocumentDegraded, shouldHideFrame, frameDegradedField, extension creation |
| `statementGutter.test.ts` | 171 | 15 tests — docVersionField, extension creation, execution integration, range markers |

### Total: 47 tests (all passing)

## Requirements coverage

| Brief step | Status | Evidence |
|------------|--------|----------|
| 1. Frame consumes unified active statement range (S2-A) | ✅ | `statementFrame.ts` imports `statementIndexField` from S2-A |
| 2. Frame hidden during multi-line non-empty selection | ✅ | `shouldHideFrame()` pure function + `buildFrameDecorations()` |
| 3. Gutter shows play on first executable line | ✅ | `getFirstExecLines()` + `PlayMarker` in `statementGutter.ts` |
| 4. Frame degraded above 500 lines | ✅ | `frameDegradedField` + `SetDegradedEffect` + tests |
| 5. Click marker generates SqlExecutionTarget(source='gutter') | ✅ | `EditorView.domEventHandlers` + `GutterClickCallback` |
| 6. Running spinner matched by documentVersion + targetRange | ✅ | `docVersionField` + `matchesRunningTarget()` |
| 7. Platform-aware tooltip | ✅ | `executeShortcutLabel()` uses `isMacOS()` |
| 8. Theme tokens, Web tooltips | ✅ | CSS custom properties + title attribute (no Tauri Menu) |

## Test results

```
npx vitest run src/components/sql-editor/extensions/ --reporter=verbose
Test Files  3 passed (3)
     Tests  47 passed (47)
```

Full sql-editor suite:
```
npx vitest run src/components/sql-editor/ --reporter=verbose
Test Files  15 passed (15)
     Tests  227 passed (227)
```

## Commit

`2c57b088d` — feat(sql-s4-a): statement frame, gutter & execution state extensions
