# Data Explore — Product Requirements Document (PRD)

> Status: Draft  
> Branch: `docs/data-explore`  
> Related: Data browsing / table viewing / data editing experience enhancement

## 1. Background & Problem Statement

DataZen currently provides a **Virtualized DataTable** with sort, filter, pagination, in-grid editing, and CSV/JSON/SQL import/export. Query results can be switched to charts. This covers the core “query → inspect result” loop for developers.

Compared with mature commercial clients (especially Navicat), gaps remain in pure **data browsing & editing** experience:

| Area | Current | Gap vs Navicat-class tools |
|------|---------|----------------------------|
| Views | Grid only | No Form view, limited Tree/JSON for document stores |
| Special types | Basic cell edit | Weak Hex / Image / large text / BLOB handling |
| FK & relations | Minimal | No FK dropdown selection or “jump to related row” |
| Understanding | Charts from result | No Data Profiling / column statistics / distribution |
| Interaction | Sort / filter / page | Weaker column management, find-replace, keyboard nav, sticky columns |
| Editing safety | Transaction controls + Safe Mode | Good foundation; still room for validation & batch helpers |

**Goal**: Close the highest-impact gaps in data browsing so that daily “open table → explore → edit a few rows” feels competitive, while staying true to DataZen’s lightweight + AI-native positioning.

## 2. Goals & Non-Goals

### 2.1 Goals

1. **Primary**: Make table / result-set browsing and light editing feel productive and trustworthy for backend & full-stack developers.
2. **Secondary**: Add lightweight data understanding (profiling) that feeds both human users and AI context.
3. **Tertiary**: Lay groundwork for Form view and richer type-specific editors without bloating the default installer.

### 2.2 Non-Goals (this initiative)

- Full visual Query Builder or Aggregate Builder (out of scope).
- Complete parity with Navicat Enterprise feature matrix.
- Heavy BI dashboarding (already covered by Ops Dashboard + charts).
- Changing the core Virtualized DataTable architecture unless required for performance or correctness.

## 3. User Personas & Jobs-to-be-Done

| Persona | Typical job | Pain today |
|---------|-------------|------------|
| Backend / full-stack dev | Open table, check a few rows, fix a value, copy IDs | Grid is usable but Form view missing for wide tables; FK lookup is manual |
| Data-curious engineer | Understand distribution / null rate before writing SQL | Must run extra `SELECT count... GROUP BY` or export to Excel |
| On-call / incident | Quickly scan recent rows, filter by status | Filtering works; sticky columns + better keyboard nav would help |

## 4. Success Metrics

- Users can open a table (≥ 100k rows) and filter + sort + edit a cell without noticeable lag (virtualization already present).
- Form view available for single-row focused editing.
- One-click column profile (null %, distinct, min/max/top-k) available on any result set or table sample.
- FK columns offer value picker when the referenced table is known from schema.
- Qualitative: internal dogfooding + community feedback that “browsing feels closer to Navicat / TablePlus for daily use”.

## 5. Scope & Priorities

### P0 — Must have (first deliverable)

1. **Enhanced Grid**
   - Sticky / freeze columns
   - Column show/hide + reorder (persisted per connection + table when possible)
   - Multi-column sort
   - Improved filter UI (null / not null, range, contains, regex where safe)
   - Find in current result (and optionally replace for editable grids)

2. **Form View**
   - Toggle Grid ↔ Form for current row
   - Keyboard navigation between fields and rows
   - Support for long text fields (expandable editor)

3. **Basic type-aware cells**
   - JSON pretty / tree toggle for JSON columns
   - Large text → side panel or modal editor
   - Boolean / enum friendly rendering

### P1 — Should have

4. **Data Profiling (lightweight)**
   - Per-column: null count / %, distinct count, min / max / top-N values
   - Optional histogram for numeric / date
   - “Profile current result” and “Profile table (sample / full with limit)”
   - Export profile as Markdown / JSON for AI context

5. **Foreign-key helpers**
   - Detect FK from schema metadata
   - Dropdown or searchable picker for FK values
   - “Open related row” action

### P2 — Nice to have

6. Image / binary preview (when content-type or extension is recognizable)
7. Hex viewer for binary columns
8. Saved filter / view presets per table
9. AI “explain this selection / find anomalies” entry points from the grid

## 6. Constraints & Principles

- **Keep it light**: Prefer progressive enhancement on the existing Virtualized DataTable; avoid a second heavy grid library if possible.
- **Respect Safe Mode / read-only connections**: Editing features must be gated.
- **Schema-aware but resilient**: Features that depend on FK / type metadata degrade gracefully when metadata is incomplete.
- **AI synergy**: Profiling output and selected cell/row context should be easy to inject into AI Chat / NL2SQL.
- **No account / no cloud**: All computation stays local.

## 7. Open Questions

1. Form view: single shared component vs driver-specific form layouts?
2. Profiling sample size defaults (e.g. 10k rows) and whether full-table profile needs explicit user confirmation.
3. Persistence key for column layout: `(connectionId, schema, table)` or include query hash for result sets?
4. How far to go on MongoDB / Redis specific explorers in the same initiative vs separate tracks.

## 8. References

- Existing: Virtualized DataTable (sort / filter / paginate / in-grid edit)
- Competitor reference: Navicat Data Viewer / Editor (Grid + Form + Profiling + FK selection)
- Related docs: architecture/frontend/components.md, large-result performance notes
