# Data Explore — Design Document

> Status: Draft  
> Depends on: [prd.md](./prd.md)

## 1. Overview

This document describes the UX and high-level technical design for improving DataZen’s data browsing experience. It builds on the existing **Virtualized DataTable** and result-panel infrastructure.

Key surfaces:

1. **Result / Table Grid** (primary)
2. **Form View** (secondary, toggle)
3. **Column Profile Panel** (side or bottom drawer)
4. **Cell / Field Editors** (inline + modal / side panel)

## 2. Information Architecture

```
Query Panel / Table Browser
├── Toolbar
│   ├── View toggle: Grid | Form
│   ├── Filter / Sort controls
│   ├── Columns menu (show/hide, freeze, reorder)
│   ├── Profile button
│   └── Find (Ctrl/Cmd+F)
├── Main content
│   ├── Grid mode → Virtualized DataTable
│   └── Form mode → Single-row form + row navigator
└── Optional panels
    ├── Column Profile drawer
    ├── Large-value editor panel
    └── FK picker popover
```

## 3. Grid Enhancements

### 3.1 Column management

- **Freeze**: left-side freeze of 0–N columns; visual separator.
- **Visibility**: column menu with checkboxes; “Show all” / “Hide empty”.
- **Reorder**: drag header or explicit “Move left/right”.
- **Persistence**: store layout under key roughly `(connectionId, objectId)` for table opens; for ad-hoc query results use ephemeral or query-hash optional save.

### 3.2 Sorting & filtering

- Multi-sort: click header cycles asc → desc → off; Shift+click adds secondary sorts; indicator shows priority.
- Filter bar or per-column filter:
  - Operators: equals, contains, starts with, is null / not null, between (numeric/date), regex (opt-in).
  - Filters apply client-side on current result page/window when data is fully loaded; for large/streamed results prefer pushing filter into SQL when “Apply to query” is chosen.

### 3.3 Find

- In-result search with highlight and next/prev.
- Optional “Replace” only when the grid is in editable mode and the connection is not read-only.

### 3.4 Virtualization & performance

- Keep existing row virtualization.
- Column virtualization only if profile shows problems with very wide tables (> ~80 columns); otherwise fixed column rendering with horizontal scroll is acceptable.
- Sticky header + frozen columns must compose correctly with virtualization.

## 4. Form View

### 4.1 Layout

- One record at a time.
- Fields laid out in a single column or two-column responsive grid based on field count and window width.
- Label + control; long text / JSON get multi-line controls with expand.
- Navigator: previous / next row, jump to row number, “new row” (if writable).

### 4.2 Behavior

- Switching Grid ↔ Form preserves current row identity.
- Dirty state tracked; leave confirmation if uncommitted changes (respect existing transaction model).
- Keyboard: Tab / Shift+Tab between fields; Ctrl+↑/↓ change row; Esc cancel edit on field.

### 4.3 Field rendering

Reuse the same type-aware cell editors as the grid (see §5) so behavior stays consistent.

## 5. Type-aware Cell & Field Editors

| Type family | Grid cell | Expanded editor |
|-------------|-----------|-----------------|
| Scalar (int, float, bool, short string) | Inline | — |
| Long text / CLOB | Truncated + expand icon | Side panel or modal with monospace + wrap toggle |
| JSON / JSONB | Truncated / badge | Tree or pretty JSON editor |
| Date / time | Locale-aware display | Date/time picker when editing |
| Binary / BLOB | Size + type hint | Optional Hex viewer (P2); download |
| FK | Value + link icon | Searchable picker (P1) |

Editors must honor read-only / Safe Mode.

## 6. Data Profiling

### 6.1 UX

- Toolbar “Profile” → opens drawer or split panel.
- Scope selector: “Current result” | “Table sample (N rows)” | “Full table (confirm)”.
- Table of columns with: name, type, null %, distinct, min, max, top-K values.
- Optional mini histogram for numeric / temporal columns.
- Actions: “Copy profile as Markdown”, “Send to AI Chat”.

### 6.2 Computation model

- Prefer pure client-side stats on already-fetched result sets.
- For table-level profile, run a controlled SQL sample (e.g. `TABLESAMPLE` / `LIMIT` + approximate distinct when available) or full scan with user confirmation and progress.
- Never block the UI; run in background worker / async task with cancel.

### 6.3 AI integration

Profile summary is a first-class context object that can be attached to AI Chat or used by NL2SQL for better column understanding.

## 7. Foreign Key Helpers (P1)

- Schema layer already exposes FK metadata where drivers support it.
- In grid/form, FK columns show a small “lookup” control.
- Picker: searchable list of candidate values from the referenced table (paginated / limited).
- “Open related” opens the referenced table filtered to that key (or jumps to row if single).

Graceful degradation when FK metadata is missing: treat as ordinary column.

## 8. State & Persistence

Suggested frontend state (Zustand or existing result-panel store):

```ts
interface DataExploreViewState {
  viewMode: 'grid' | 'form';
  currentRowId: string | number | null;
  columnLayout: {
    order: string[];
    hidden: string[];
    frozenCount: number;
    widths?: Record<string, number>;
  };
  filters: FilterSpec[];
  sorts: SortSpec[];
  profile: ProfileResult | null;
  profileStatus: 'idle' | 'running' | 'error';
}
```

Persist `columnLayout` (and optionally saved filters) via existing store / local preference layer keyed by connection + object identity.

## 9. Accessibility & Keyboard

- Full keyboard operation for grid navigation, form fields, and profile panel.
- Focus management when switching Grid ↔ Form.
- Screen-reader friendly column headers and form labels.

## 10. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Frozen columns + virtualization bugs | Early spike; visual regression tests |
| Profile on huge tables freezes UI | Strict sampling defaults + cancel + progress |
| Layout persistence conflicts across machines | Keep local-only; document key scheme |
| Form view inconsistent with driver types | Shared editor components driven by normalized column metadata |

## 11. Out of Scope for Design (see Implementation)

Detailed component APIs, file paths, IPC shapes, and test plans are covered in [implementation.md](./implementation.md).
