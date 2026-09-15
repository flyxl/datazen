# Data Explore — Implementation Plan

> Status: Draft  
> Depends on: [prd.md](./prd.md), [design.md](./design.md)

## 1. Principles

- Prefer extending the existing Virtualized DataTable and result-panel code paths over introducing a second grid stack.
- Ship behind progressive UI (no feature flag required for P0 if low risk; use flag only for experimental profiling defaults).
- Keep all heavy work off the main thread; respect existing streaming / large-result patterns.
- Align with docs policy: this folder is temporary planning; once features land on main, move stable usage notes to `docs/features/` and architecture facts to `docs/architecture/`.

## 2. Suggested Phasing

### Phase 0 — Discovery & spikes (short)

- [ ] Inventory current DataTable / result panel components and state (see `architecture/frontend/components.md` and large-result notes).
- [ ] Spike: frozen columns + virtualization composition.
- [ ] Spike: Form view layout with shared cell editors.
- [ ] Confirm schema metadata available for FK and types per driver (PG / MySQL / SQLite first).

### Phase 1 — P0 Grid + Form (core browsing)

**Frontend**

- [ ] Column layout model: order, hidden, frozenCount, widths.
- [ ] Column menu UI + drag-reorder (or button-based reorder if drag is costly).
- [ ] Sticky/frozen column rendering integrated with virtualizer.
- [ ] Multi-sort indicators and Shift+click behavior.
- [ ] Improved filter UI (null / range / contains); wire to existing client filter or “rewrite query” action.
- [ ] Find-in-result (highlight + next/prev).
- [ ] View mode toggle Grid | Form.
- [ ] Form view component: field list, row navigator, dirty tracking, keyboard nav.
- [ ] Shared type-aware editors for long text and JSON (inline expand + side panel).

**State / persistence**

- [ ] Extend result / table view store with `viewMode`, `columnLayout`, `filters`, `sorts`.
- [ ] Persist column layout to local preferences keyed by connection + object identity.

**Testing**

- [ ] Unit tests for layout reducers and filter/sort pure logic.
- [ ] Component tests for Form ↔ Grid switch preserving row.
- [ ] Manual / e2e: open large table, freeze columns, filter, edit cell, switch to Form.

### Phase 2 — P1 Profiling + FK helpers

**Profiling**

- [ ] Client-side profile engine for in-memory result sets (null %, distinct, min/max, top-K).
- [ ] Optional SQL sample path for table-level profile (`LIMIT` / dialect-specific sample).
- [ ] Profile drawer UI + progress / cancel.
- [ ] “Copy as Markdown” and “Send to AI Chat” actions.

**FK**

- [ ] Consume existing FK metadata from schema layer.
- [ ] FK cell affordance + searchable picker (paginated lookup query).
- [ ] “Open related” navigation.

**Testing**

- [ ] Profile correctness on fixture tables.
- [ ] FK picker with and without metadata (graceful fallback).
- [ ] Performance check: profile on 50k-row result stays responsive.

### Phase 3 — P2 polish

- [ ] Image / binary preview heuristics.
- [ ] Hex viewer (read-only first).
- [ ] Saved filter presets per table.
- [ ] AI entry points from selection (“explain selection”, “find anomalies”).

## 3. Technical Notes

### 3.1 Component ownership (proposed)

| Concern | Likely location |
|---------|-----------------|
| Virtualized grid core | Existing DataTable / result grid package |
| Column layout state | Result panel / table workspace store |
| Form view | New sibling component under same feature folder |
| Cell editors | Shared editors module used by Grid + Form |
| Profile engine | Pure TS utility + thin UI shell; optional worker |
| FK lookup | Schema service + small query helper via existing driver commands |

Exact paths should be confirmed against current `src` / `packages/ui` layout during Phase 0.

### 3.2 Backend / IPC

- Prefer **no new IPC** for pure client-side profile on existing results.
- Table sample profile may reuse existing query execution path with a generated SQL sample.
- FK value lookup = ordinary `SELECT` with limit + filter; no special driver API required if metadata is already present.

### 3.3 Driver coverage order

1. PostgreSQL, MySQL/MariaDB, SQLite (default drivers)
2. SQL Server / others when metadata parity exists
3. MongoDB / Redis: separate explorer improvements (not blocking relational grid/form work)

### 3.4 Safety

- All write paths must check read-only connection and Safe Mode.
- Replace / batch edit only when editable.
- Profile full-table scan requires explicit confirmation and cancel.

## 4. Acceptance Checklist (P0)

- [ ] Can freeze ≥ 1 column and scroll horizontally/vertically without misalignment.
- [ ] Can hide/show/reorder columns; layout restored on reopen for same table.
- [ ] Multi-sort works with visible priority indicators.
- [ ] Filter by null / contains / range on current result.
- [ ] Find locates and cycles matches in the current result.
- [ ] Toggle to Form view on current row; edit a field; navigate rows; return to Grid on same row.
- [ ] Long text and JSON have usable expanded editors.
- [ ] Read-only / Safe Mode: no edit affordances that would write.

## 5. Acceptance Checklist (P1)

- [ ] Profile current result shows per-column null %, distinct, min/max/top-K.
- [ ] Profile can be copied as Markdown and sent to AI Chat.
- [ ] FK column shows picker when metadata exists; picker inserts value on select.
- [ ] “Open related” navigates sensibly.

## 6. Documentation follow-up (after merge to main)

- Add user-facing notes under `docs/features/` (e.g. data-explore or table-viewer guide).
- Update `architecture/frontend/components.md` with Form view, profile, and layout persistence facts.
- Remove or archive this `docs/todo/data-explore/` folder per project docs policy once the work is no longer “todo”.

## 7. Open implementation questions

1. Exact preference storage key scheme and migration if layout shape changes.
2. Whether profile histograms need a chart dependency or simple CSS/SVG bars.
3. Worker vs async main-thread for profile on medium results (measure first).
4. Form view density settings (compact vs comfortable) — default?

---

*This plan is intentionally lightweight so it can be revised quickly during implementation.*
