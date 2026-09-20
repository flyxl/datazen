# Visual Query Builder

> Status: **v3.5** — Navicat-style statement list, covered by four E2E journeys.
> v3.5: **one chip per item in every clause** — clicking a chip (column, table,
> condition, group-by key, order-by key) opens that item's dialog, its × removes
> it. FROM tables and WHERE/HAVING conditions became chips too.
> v3.4: the Build tab is one row per clause (SELECT / FROM / WHERE / GROUP BY /
> HAVING / ORDER BY), per-column options live in a dialog, and **HAVING** exists.
> v3.3: fixed-height cards that scroll internally, orthogonal direction-less
> relation lines, and a preview that is highlighted, pretty-printed and full
> height.
> PRD: [docs/prd/query-builder-prd.md](../prd/query-builder-prd.md)

## Overview

The Visual Query Builder builds a SQL `SELECT` graphically — tables, joins,
columns, aliases, aggregates, WHERE (including nested groups), GROUP BY,
ORDER BY, DISTINCT and LIMIT/OFFSET — and hands the generated statement to the
SQL editor. **It never executes anything itself**; running the query stays the
editor's job.

## How to Access

1. Open a database connection and open a **Query** tab.
2. Toolbar → **More** menu (`query-toolbar-more-menu-trigger`) → **Visual Builder**
   (`more-menu-visual-builder`).
3. The builder **replaces the query content area** (it is not stacked above the
   editor). The left `ConnectionNavigatorTree` stays visible and doubles as the
   builder's object source — the builder has no tree of its own.

Closing: **OK** commits to the editor, **Cancel** / **×** discard and return to
the editor, and the More-menu entry toggles the builder hidden/shown without
discarding (the canvas state survives).

## Layout

```
┌──────────────────────────────────────────────┐
│ header  [title] [DISTINCT] [reset] [⤢] [×]   │
├──────────────────────────────────────────────┤
│ canvas (flex-1, collapsible)                 │
├══ splitter (draggable, persisted) ═══════════┤
│ bottom tabs  [ Build | Preview ]             │
├──────────────────────────────────────────────┤
│ footer                          [Cancel][OK] │
└──────────────────────────────────────────────┘
```

- **Build** and **Preview** are mutually exclusive — only the active tab is
  mounted, which is what keeps the canvas tall enough to work in.
- The splitter between canvas and tabs is draggable; the height persists under
  `localStorage["resize:qb-split-height"]`. The default is 360px (was 300px:
  at the old default, a four-column statement pushed GROUP BY below the fold on
  a 960×720 window — measured, see journey D's `clauseIsVisible` assertions).
- The canvas collapses from the header button or `Ctrl/Cmd + B`.

### The Build tab is a statement list

Navicat-style: **one row per SQL clause**, a keyword gutter on the left and that
clause's content beside it.

```
SELECT   [ ] DISTINCT  ⟨SUM(s.qty) AS total_qty⟩ ⟨s.price⟩      <Click here to add fields>
FROM     ⟨sales AS s⟩ ⟨INNER JOIN regions AS r⟩                 <Click here to add tables>
WHERE    ⟨s.status = 'paid'⟩ ⟨OR r.name LIKE 'E%'⟩              <Click here to add conditions>  [+ Add Group]
GROUP BY ⟨r.name⟩                                               <Click here to add GROUP BY>
HAVING   ⟨SUM(s.qty) >= 2000⟩                                   <Click here to add conditions>
ORDER BY ⟨r.name ASC⟩                                           <Click here to add ORDER BY>
         Limit [   ]  Offset [   ]                              ← pinned toolbar row
```

> **One rule for every clause.** Each item is a chip: it shows what is in the
> query, clicking it opens that item's dialog, and its × removes it. A join chip
> is badged with its `JOIN` type; a condition chip is badged with the `AND`/`OR`
> that links it to the row above; a table no join reaches is marked
> "not joined to the query yet". A nested condition group is a bordered box whose
> own logic selector sits in front of its chips.

This replaced an Excel-like grid (`CriteriaGrid` + `CriteriaRow`) that spent one
row per selected column on eight inline controls (field, table, alias, sort,
function, where, group, delete). Four selected columns filled the whole region
and pushed WHERE — and everything below it — out of sight. A clause now takes
only the height of the content it actually holds, a field is one chip, and the
per-field controls moved into a dialog (see *Column Options*).

## Features

### Table Selection

- Drag a **table or view** from the connection navigator onto the canvas.
  Dragging is the _only_ way a table enters the builder.
- **Clicking** a navigator entry always opens that table's data view, whether or
  not the builder is open — the builder never hijacks navigation.
- New cards are grid-snapped onto the row they were dropped into, so several
  drops line up instead of landing a few pixels apart.

### Table Cards

- **Move** a card by dragging anywhere on it (the header is the natural grab
  point); positions snap to a 24px grid and align to a neighbouring row, so
  cards can be tidied by hand.
- **Remove** a table with the card's `×`. That also drops its selected columns,
  its WHERE conditions (nested ones included), its JOINs, its alias and its
  position — nothing is left referencing a deleted table.
- **Select all columns** with the checkbox in the card header (indeterminate
  when only some are selected). Unchecking clears every column of that table.
- **Alias**: every table gets a default alias as soon as it is added
  (`film_actor` → `fa`, unique per query). The alias field edits it.

> Depends on the desktop runtime forwarding HTML5 drag events to the webview:
> every programmatically created window must call `disable_drag_drop_handler`
> (see [e2e-coverage → macOS 原生拖放验收](../development/e2e-coverage.md)).

### Column Selection

- Toggle individual columns on/off from the table card, or all at once. A
  selected column appears as one chip in the SELECT row.
- Table aliases are **used everywhere**: the alias is declared once in
  `FROM`/`JOIN` and then qualifies every column in `SELECT`, `WHERE`, `HAVING`,
  `GROUP BY`, `ORDER BY` and `ON`. The chips and condition editors label fields
  with that same qualifier, so what the builder shows is what the SQL emits.
  (Previously an alias appeared in `ON` only, which made adding one pointless.)

### Item dialogs

Every chip opens a dialog holding that item's options:

| Chip           | Dialog                                                                             |
| -------------- | ---------------------------------------------------------------------------------- |
| SELECT field   | **Column options**: alias, aggregate, sort, group by, one criteria condition         |
| FROM table     | **Table options**: alias (editable); join type and `ON` predicate shown read-only   |
| WHERE / HAVING | **Condition**: field, (HAVING only) aggregate, operator, value, `AND`/`OR`           |
| GROUP BY key   | opens that column's **Column options** (only when the column is also in SELECT)      |
| ORDER BY key   | **Sort options**: `ASC` / `DESC`                                                     |

Editing is **draft-only until OK**: the form is seeded once per opened item and
cancel writes nothing. That matters most for a **new condition** — it exists only
as a draft, so abandoning the dialog cannot leave a half-filled row behind that
would both render an empty chip and block OK with `empty-condition-value`.

The join type and its `ON` predicate are deliberately **read-only** in the table
dialog: they belong to the relation group drawn on the canvas (and to
`buildJoinSteps`), and two editors of one composite key could disagree.

### Relations (JOINs)

Foreign keys are drawn as **lines only — the canvas carries no relation text**.
Each line connects the two specific columns it relates, at that column's row.

- **One constraint = one line object.** A single-column FK is an orthogonal
  polyline; a **composite FK** merges its source stubs into one trunk and splits
  the trunk into one stub per referenced column (`many → one → many`). Two
  constraints between the same pair of tables get **parallel lanes** 14px apart,
  ordered by source row index so they never swap while dragging.
- **State**: dashed + muted = detected but not in the SQL; solid + accent = in
  the SQL. A half-confirmed composite FK is drawn half-solid and blocks OK via
  the `composite-join-incomplete` diagnostic.
- **Hover** brightens the line and highlights every column row it touches, so
  "which two columns?" is answerable without a label.
- **Click** opens a small popover at that point: `INNER / LEFT / RIGHT / FULL`
  plus _add to query_ (candidate) or _remove from query_ (confirmed). A
  composite group is always confirmed/removed as a whole — half of a composite
  key is a wrong query.
- **Manual joins**: hover a column row and drag its connector dot onto another
  column to create a join by hand. Manual joins are drawn distinctly (dash-dot,
  secondary tone). Dragging within one table is refused (self joins cannot be
  expressed).
- Self-referencing FKs are drawn as a loop and are not confirmable, for the same
  reason.

> Colour comes from the semantic `--c-*` tokens. An earlier revision referenced
> a `--color-*` namespace that does not exist, which made every connector render
> with an invalid `stroke` — i.e. the lines were invisible.

> **Geometry.** Every relation is an **orthogonal polyline** (every segment is
> horizontal or vertical) and carries **no direction**: there are no arrowheads
> and no `marker-*` on any path — FK direction is a fact about the DDL, not a
> question the canvas is answering. Each pair terminates in a symmetric dot at
> both ends. If a column is scrolled out of its card's list, its anchor is
> clamped to the list edge and drawn as a **hollow ring** (never a chevron or
> arrowhead, which would read as a direction) instead of pointing at a row that
> is not there.

> **Card height is fixed.** A card is `header + min(list, 200px)`; a table with
> more columns than that **scrolls its own column list** (Navicat behaviour)
> rather than growing, so a wide table can never push its neighbours off screen.
> The canvas itself scrolls natively (wheel/trackpad or the scrollbars) and zooms
> with **Ctrl/Cmd + wheel** around the pointer; panning is scrolling
> (middle-drag or Space + drag).

### WHERE Conditions

- The WHERE row holds root conditions plus nested groups
  (`qb-where-add-condition` / `qb-where-add-group`), one level of nesting; the
  empty state is the Navicat `<Click here to add conditions>` link.
- Per-column criteria set in a column's options dialog are shown as chips above
  the tree and removed from the chip — the generator merges them into the same
  clause, so hiding them would lose the user's own filter.
- Operators: `=`, `!=`, `>`, `<`, `>=`, `<=`, `LIKE`, `NOT LIKE`, `IN`,
  `NOT IN`, `IS NULL`, `IS NOT NULL`.
- `IN` / `NOT IN` take a comma-separated list; `IS NULL` / `IS NOT NULL` hide
  the value input.
- Nested groups are parenthesised in the generated SQL (`a AND (b OR c)`).
- Each row's own AND/OR links it to the row above it. (It used to be ignored in
  favour of the group's logic, so a row set to `OR` was emitted as `AND`.)

### GROUP BY / HAVING / ORDER BY / DISTINCT / LIMIT-OFFSET

- **GROUP BY** and **ORDER BY** each have their own row with a picker
  (`qb-add-group-by` / `qb-add-order-by`). They show the union of the dedicated
  store list and the per-column flags, and removing a chip clears whichever
  source owns it. Clicking an ORDER BY chip flips `ASC` / `DESC`.
- **HAVING** filters aggregated groups and is emitted between GROUP BY and
  ORDER BY. Its condition rows carry a per-row **aggregate** selector, and a new
  row starts as `SUM(...)`, so `HAVING SUM(qty) >= 2000` is two clicks.
  A HAVING operand that is neither aggregated nor grouped raises the
  `having-non-grouped` **warning** (see Validation) — it is valid to type but
  rejected by PostgreSQL/MySQL/SQL Server at execution time.
- Sorting an **aggregated** column orders by the aggregate expression
  (`ORDER BY SUM(x) DESC`), not the bare column — the latter is rejected by
  every engine once the column is aggregated but not grouped.
- `DISTINCT` toggles at the start of the SELECT row.
- `LIMIT` / `OFFSET` are numeric inputs pinned to the top of the Build tab.
  An offset without a limit emits `OFFSET n` (PostgreSQL/SQLite via `LIMIT -1`;
  MySQL uses its unbounded sentinel) instead of the old `LIMIT 0`, which
  silently returned zero rows.

### Validation

Invalid states are **named and blocked** rather than emitted as confident-looking
SQL. `validateQuery` (`components/query-builder/validation.ts`) reports:

| Diagnostic                                | Blocks OK | Why                                                                                                   |
| ----------------------------------------- | --------- | ----------------------------------------------------------------------------------------------------- |
| `no-tables` / `no-columns`                | yes       | Nothing to build                                                                                      |
| `empty-condition-value`                   | yes       | `col = NULL` is never true                                                                            |
| `empty-in-list`                           | yes       | `IN ()` is a syntax error                                                                             |
| `invalid-limit`                           | yes       | `LIMIT -5` is a syntax error                                                                          |
| `unsupported-pagination`                  | yes       | SQL Server cannot express LIMIT/OFFSET here; dropping it silently would return a different result set |
| `alias-duplicate` / `alias-shadows-table` | yes       | Ambiguous identifier references                                                                       |
| `unknown-join-table` / `self-join`        | yes       | Join cannot be expressed as drawn                                                                     |
| `composite-join-incomplete`               | yes       | Half of a composite key is a wrong query                                                              |
| `having-non-grouped`                      | **no**    | HAVING operand is neither aggregated nor grouped — rejected at execution, but still buildable          |

Diagnostics appear above the SQL in the Preview tab, dot the Build tab, and
disable OK (with the reason in its tooltip).

Values are typed conservatively: only literals that survive `String(Number(v))`
unchanged are emitted unquoted, so `007` stays `'007'` and `1.50` does not
silently become `1.5`.

### Preview Tab

- Live SQL for the current canvas, with the active dialect badge and a copy
  button.
- **Pretty-printed**: the generator emits one long line, the preview runs it
  through the same formatter (and the same user format options) as the editor's
  "Format SQL", so `LIMIT 50` reads as a clause rather than a run-on. OK commits
  exactly the text the preview showed, and the "editor already holds this" check
  compares against the formatted text too (otherwise every reopen would look
  like a conflict).
- **Syntax-highlighted** by a small in-house tokenizer (`sqlHighlight.ts`) that
  emits one `<span>` per token; it is lossless, so the rendered text is still
  byte-identical to the generated SQL (the E2E probe reads it back via
  `textContent`).
- Fills the tab region (the preview `<pre>` grows to the available height and
  scrolls internally), instead of collapsing to a short box.
- Read-only by design: the builder is the single source of truth, so text edits
  belong in the editor after OK.
- A generation problem (for example a table with no columns selected) shows a
  dot on the **Build** tab.

### OK / Cancel

- **OK** writes the generated SQL back, closes the builder and focuses the
  editor:
  - editor empty → written directly;
  - editor content already identical → nothing is rewritten, it just closes;
  - editor content differs → a three-way prompt: **Replace** / **Append** /
    **Keep editor content** (never silently overwritten).
- **Cancel** / **×** roll the canvas back to the snapshot taken when the builder
  opened, asking for confirmation first when the canvas was modified.
- **Reset** clears the canvas but keeps the builder open.

## Keyboard Shortcuts

| Action                   | Shortcut           |
| ------------------------ | ------------------ |
| Collapse / expand canvas | `Ctrl/Cmd + B`     |
| Commit (same as OK)      | `Ctrl/Cmd + Enter` |
| Cancel (asks when dirty) | `Esc`              |

Shortcuts are active only while the builder is visible.

## Supported Operators

| Operator                  | Description                       |
| ------------------------- | --------------------------------- |
| `=`                       | Equal to                          |
| `!=`                      | Not equal to                      |
| `>` / `<` / `>=` / `<=`   | Comparisons                       |
| `LIKE`                    | Pattern match (`%`, `_`)          |
| `NOT LIKE`                | Negated pattern match             |
| `IN` / `NOT IN`           | List membership (comma-separated) |
| `IS NULL` / `IS NOT NULL` | Null checks                       |

## Supported Dialects

| Database   | Identifier Quoting | ILIKE | LIMIT/OFFSET                               |
| ---------- | ------------------ | ----- | ------------------------------------------ |
| PostgreSQL | `"column"`         | ✅    | `LIMIT n OFFSET m` (offset omitted when 0) |
| MySQL      | `` `column` ``     | ❌    | `LIMIT m, n`                               |
| SQLite     | `"column"`         | ❌    | `LIMIT n OFFSET m`                         |
| SQL Server | `[column]`         | ❌    | Not supported in v1                        |
| Generic    | `"column"`         | ❌    | `LIMIT n OFFSET m`                         |

## E2E Coverage

Four journeys, registered in the `query-builder` suite:

```bash
pnpm e2e:qb              # run all of them (uses the existing debug build)
pnpm e2e:qb:build        # build first
pnpm e2e:qb:regression   # blast-radius guard: query panel / editor / navigator
```

| Journey             | Spec                                                          | Covers                                                                                                                                                |
| ------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| A — normal          | `e2e/specs/journeys/visual-query-builder-journey.ts`          | open from navigator → columns → tabs → splitter → collapse → WHERE → DISTINCT → OK → execute → reset → close; **A17–A20**: fixed-height card with an internally scrolling column list, height unchanged by scrolling, preview formatted / highlighted / filling the tab |
| B — abnormal        | `e2e/specs/journeys/visual-query-builder-edge-journey.ts`     | empty state, OK never executes, replace/append/keep conflict paths, cancel rollback, no-relation hint, panel isolation                                 |
| C — high complexity | `e2e/specs/journeys/visual-query-builder-complex-journey.ts`  | 3-table FK joins + LEFT re-type + aggregate/alias + GROUP BY + ORDER BY + DISTINCT + nested `AND (… OR …)` + IN list + LIMIT/OFFSET, then executes it; **C16**: composite FK as one trunk — axis-aligned segments, zero direction markers, a dot at both ends of each pair |
| D — clauses         | `e2e/specs/journeys/visual-query-builder-clauses-journey.ts`  | all six clause rows exist; WHERE/GROUP BY stay on screen with four columns selected and the whole statement fits once the canvas is collapsed; column options dialog (cancel writes nothing, OK applies); GROUP BY + ORDER BY pickers and the sort dialog's ASC/DESC; **HAVING exists only as a draft until OK** and `SUM(qty) >= 5` is committed and executed, filtering a group out of the result; `having-non-grouped` warns without blocking OK |

Shared drivers live in `e2e/specs/journeys/visualQueryBuilderHelpers.ts`.

Each journey brings up its **own disposable connection** and seeds its tables
through the live panel. That is deliberate: the runner recreates the worker
database between spec files, so reusing the shared `conn_e2e_pg` config can
hand a spec a session pointing at a database a previous spec just dropped.

## Lifecycle

The builder belongs to the query panel that opened it:

- Closing a query panel **tab** destroys its builder — the next panel starts
  from a blank canvas, never a leftover one.
- Switching tabs does _not_ destroy it: an inactive panel is unmounted, and its
  canvas (plus selected tab and splitter height) is restored when you return.
- Two query panels never share a canvas.

## Architecture

- **State**: Zustand `queryBuilderStore` — canvas state plus panel-scoped view
  state (`openPanelId`, `bottomTab`, `canvasCollapsed`) and an entry snapshot
  used for the dirty check and cancel rollback.
- **SQL generation**: pure `generateSql` in
  `components/query-builder/hooks/useSqlGenerator.ts`.
- **Join ordering**: `generateJoinClause` in `lib/sqlDialects/queryBuilder.ts`.
- **Dialect adaptation**: `QbDialectAdapter`.
- **Integration**: `QueryEditorSection.tsx` hosts the builder and owns the
  commit/focus/toast behaviour; `QueryPanel.tsx` yields the result pane while
  the builder is up.
- **i18n**: all UI text uses `query.visualBuilder.*` (source of truth:
  `src/locales/en/query.ts`).

## Known Limitations

- SQL Server LIMIT/OFFSET not supported (uses TOP/OFFSET-FETCH).
- Subqueries, HAVING, UNION and window functions are not supported.
- Self-joins cannot be expressed (no table alias instances).
- The builder's canvas state is not persisted across app restarts.
