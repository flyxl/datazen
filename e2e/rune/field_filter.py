"""Physical SQLite Table Data Field Filter journey; no server or user data."""
import json
import sqlite3

from offline import OPEN, key
from scenarios import fixture, tap, type_text, visible


class TableFieldFilterJourney:
    def test_sqlite_table_field_filter_projects_raw_cells_and_preserves_tab_scope(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            seed.execute("ALTER TABLE customers ADD COLUMN city TEXT DEFAULT 'Zürich'")
            for at in range(14):
                seed.execute(f"ALTER TABLE customers ADD COLUMN metric_{at} INTEGER DEFAULT {at}")
            seed.execute("CREATE TABLE orders AS SELECT * FROM customers")
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        settings = self.root / "settings.json"
        settings.write_text(json.dumps({"ui": {"language": "en"}, "rowCountMode": "exact",
                                        "defaultTableSort": "primaryKeyAsc"}), encoding="utf-8")
        before = [path.read_bytes() for path in (self.db, self.store, settings)]

        trigger = "table-column-filter-toggle"
        popup = "table-column-filter-popover"
        search = "table-column-filter-search"
        count = "column-filter-count-badge"
        hide_name = "column-filter-checkbox-customer_name"

        searching = [*OPEN, tap(4.8, "cell-0-1"), tap(5.3, trigger), type_text(5.9, "TEXT")]
        search_tree = self.replay(searching, until=6.5)
        self.expect_rendered(search_tree, key=popup)
        self.expect_rendered(search_tree, key=search)
        self.expect_rendered(search_tree, key=hide_name)
        self.expect_rendered(search_tree, key="column-filter-checkbox-city")
        self.assertFalse(visible(search_tree, key="column-filter-checkbox-id"))
        pop = self._settled_rect(search_tree, popup)
        anchor = self._settled_rect(search_tree, trigger)
        self.assertGreaterEqual(pop[1], anchor[1] + anchor[3])
        self.assertAlmostEqual(pop[0] + pop[2], anchor[0] + anchor[2], delta=12)

        hidden = [*searching, tap(6.7, hide_name), key(7.3, "Escape")]
        hidden_tree = self.replay(hidden, until=7.95)
        self.expect_rendered(hidden_tree, key=count, text="16/17")
        self.assertFalse(visible(hidden_tree, key=popup))
        self.assertFalse(visible(hidden_tree, key="head-1"))
        self.assertFalse(visible(hidden_tree, key="cell-0-1"))
        self.expect_rendered(hidden_tree, text="Zürich", within="cell-0-2")
        self.expect_rendered(hidden_tree, key="grid-filter-count", text="2 rows")
        self.expect_rendered(hidden_tree, key="grid-page-label", text="Page 1 / 1")
        self.expect_rendered(hidden_tree, key="data-grid-status",
                             text="2×16 · window at column 0 · selection none · 0 dirty")
        first, city = (self._settled_rect(hidden_tree, f"head-{i}") for i in (0, 2))
        self.assertAlmostEqual(first[0] + first[2], city[0], delta=.25)

        selected = [*hidden, tap(8.2, "cell-0-0"), key(8.65, "Right")]
        navigated = self.replay(selected, until=9.25)
        self.expect_rendered(navigated, key="data-grid-status",
                             text="2×16 · window at column 0 · selection r0c2..r0c2 · 0 dirty")
        self.assertFalse(visible(navigated, key="grid-editor"))
        clear = [*selected, tap(9.55, "rownum-1"), tap(10.0, trigger)]
        open_all = self.replay(clear, until=10.55)
        self.expect_rendered(open_all, key="column-filter-pk-id", text="PK")
        self.expect_rendered(open_all, key="column-filter-checkbox-metric_0")
        zero = [*clear, tap(10.75, "table-column-filter-clear-all"), key(11.25, "Escape")]
        empty = self.replay(zero, until=11.8)
        self.expect_rendered(empty, key="all-columns-hidden-state")
        self.expect_rendered(empty, key=count, text="0/17")
        self.expect_rendered(empty, key="grid-filter-count", text="2 rows")
        self.assertFalse(visible(empty, key="data-grid-scroll"))
        reset = self.replay([*zero, tap(12.0, "table-reset-hidden-columns")], until=12.75)
        self.expect_rendered(reset, text="fixture-ada", within="cell-0-1")
        self.expect_rendered(reset, key="data-grid-status",
                             text="2×17 · window at column 0 · selection row 2 · 0 dirty")
        self.assertFalse(visible(reset, key=count))

        select_all = self.replay([
            *zero, tap(12.0, trigger), tap(12.55, "table-column-filter-select-all"),
            tap(13.1, "grid-filter-count"),  # outside pointer closes the anchored popup
        ], until=13.8)
        self.expect_rendered(select_all, text="fixture-ada", within="cell-0-1")
        self.assertFalse(visible(select_all, key=count))
        self.assertFalse(visible(select_all, key=popup))

        other = [*hidden, tap(8.2, "tree-table-orders")]
        other_tree = self.replay(other, until=9.3)
        self.expect_rendered(other_tree, text="orders", within="tab-2")
        self.expect_rendered(other_tree, key="head-1")
        self.assertFalse(visible(other_tree, key=count))
        back = self.replay([*other, tap(9.6, "tab-1")], until=10.65)
        self.expect_rendered(back, key=count, text="16/17")
        self.assertFalse(visible(back, key="head-1"))

        # Existing Freeze-through keeps the first N displayed, not hidden raw
        # columns. Test real scrolling, with ID hidden and the next raw key held.
        frozen = self.replay([
            *OPEN,
            {"t": 4.8, "type": "secondary-click", "key": "head-1"},
            tap(5.4, "table-column-freeze-item0"),
            tap(5.95, trigger), tap(6.55, "column-filter-checkbox-id"),
            key(7.05, "Escape"),
            {"t": 7.5, "type": "drag", "key": "horizontal-scrollbar/thumb", "dx": 240, "dur": .35},
        ], until=8.5)
        viewport = self._settled_rect(frozen, "data-grid-scroll")
        left = viewport[0] + 40
        self.assertFalse(visible(frozen, key="head-0"))
        for raw in (1, 2):
            header = self._settled_rect(frozen, f"head-{raw}")
            cell = self._settled_rect(frozen, f"cell-0-{raw}")
            self.assertAlmostEqual(header[0], left, delta=.25)
            self.assertAlmostEqual(cell[0], left, delta=.25)
            left += header[2]
        self.assertFalse(visible(frozen, key="head-3"), "the real scrollbar moved")
        self.assertEqual([path.read_bytes() for path in (self.db, self.store, settings)], before)
