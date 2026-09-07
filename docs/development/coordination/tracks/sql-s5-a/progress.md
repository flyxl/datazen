# Track S5-A: Multiple Selections & Paste Extension Point — Progress

## Status: READY_FOR_TEST

## Implementation Summary

### Retained Host Features
1. `src/components/sql-editor/paste/multipleSelections.ts` — Multi-cursor extension (allowMultipleSelections, Mod+D next occurrence, rectangular selection)
2. `src/components/sql-editor/paste/__tests__/multipleSelections.test.ts` — Unit tests for multi-cursor

### Pro Features Transferred
Paste as IN, Drop Caret, and Delimited Values parsing have been moved to independent private repository:
`https://github.com/flyxl/datazen-extension-sql-pro`

Host uses `sqlEditorProEP` fallback.
