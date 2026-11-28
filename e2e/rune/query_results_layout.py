"""Stock native query/result divider, retained SQLite rows and real geometry."""
import json
import sqlite3
from contextlib import closing

from scenarios import fixture, query_steps, tap, type_text, visible


class QueryResultsLayoutJourney:
    def test_pinned_query_split_resizes_real_result_grid_and_survives_restart(self):
        fixture(self.store, self.db)
        with closing(sqlite3.connect(self.db)) as db, db:
            db.executemany("INSERT INTO customers VALUES (?, ?)", [
                (i, f"split-row-{i:02}") for i in range(3, 97)
            ])
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original = self.db.read_bytes()
        settings = self.root / "settings.json"
        settings.write_text('{"ui":{"language":"en","themeMode":"light"}}', encoding="utf-8")
        settings.chmod(0o600)

        sql = "SELECT id, customer_name FROM customers ORDER BY id;"
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            rows = db.execute(sql).fetchall()
        self.assertEqual(len(rows), 96)
        execute = query_steps(sql)

        def sizes(tree):
            editor = self._settled_rect(tree, "query-editor-pane")
            main = self._settled_rect(tree, "query-main")
            host = self._settled_rect(tree, "result-workspace")
            frame = self._settled_rect(tree, "result-data-grid-scroll-scroll-area")
            viewport = self._settled_rect(tree, "result-data-grid-scroll")
            sticky = self._settled_rect(tree, "result-data-grid-header")
            divider = self._settled_rect(tree, "splitter:query-editor-height")
            self.assertEqual(divider[3], 6)
            self.assertAlmostEqual(divider[1], editor[1] + editor[3], delta=.25)
            self.assertGreaterEqual(editor[3], 120)
            self.assertAlmostEqual(sticky[1], viewport[1], delta=.25)
            self.assertGreaterEqual(viewport[3], sticky[3] + 32)
            self.expect_rendered(tree, key="result-grid-status", text="result · 96 rows × 2 cols")
            self.expect_rendered(tree, key="result-status-text")
            return editor, host, frame, main

        def fits(tree):
            editor, host, frame, main = sizes(tree)
            self.assertAlmostEqual(host[1] + host[3], main[1] + main[3], delta=.25)
            self.assertAlmostEqual(frame[1] + frame[3], host[1] + host[3], delta=.25)
            status = self._settled_rect(tree, "content-status-bar")
            self.assertAlmostEqual(status[1], main[1] + main[3], delta=.25)
            return editor, frame

        def drag(dy):
            return {"t": 10.65, "type": "drag", "key": "splitter:query-editor-height",
                    "dy": dy, "dur": .45}

        def saved_height():
            return json.loads(settings.read_bytes())["ui"].get("layout", {}).get("query-editor-height")

        idle = self.replay(execute[:-1], until=8.35)
        self.assertFalse(any(n.get("key") == "splitter:query-editor-height" for n in idle["elements"]))
        before = self.replay(execute, until=10.4)
        previous_editor, previous_frame = fits(before)
        auto_host = self._settled_rect(before, "query-results-host")
        self.assertAlmostEqual(auto_host[3], self._settled_rect(before, "query-main")[3] / 2, delta=.25)
        self.assertIsNone(saved_height())

        tall = self.replay([*execute, tap(10.35, "result-cell-1-1"), drag(-80)], until=12.0)
        editor, frame = fits(tall)
        self.assertAlmostEqual(editor[3], previous_editor[3] - 80, delta=1)
        self.assertAlmostEqual(frame[3], previous_frame[3] + 80, delta=1)
        self.expect_rendered(tall, key="result-status-text", text="Ready · 96 rows × 2 cols · selection r1c1..r1c1")
        self.assertAlmostEqual(saved_height(), editor[3], delta=1)
        self.expect_rendered(tall, text=rows[1][1], within="result-cell-1-1")
        self.assertEqual(fits(self.replay(execute, until=10.4)), (editor, frame))

        cramped = self.replay([*execute, drag(150)], until=12.0, size="1440x640")
        small_editor, minimum_frame = fits(cramped)
        self.assertGreater(saved_height(), small_editor[3], "min-height layout constrained the preference")
        self.assertLess(minimum_frame[3], frame[3])

        reopened = self.replay([*execute, drag(-24)], until=12.0, size="1440x640")
        editor_again, frame_again = fits(reopened)
        self.assertAlmostEqual(small_editor[3] - editor_again[3], 24, delta=1)
        self.assertAlmostEqual(frame_again[3] - minimum_frame[3], 24, delta=1)
        self.assertAlmostEqual(saved_height(), editor_again[3], delta=1)

        found = self.replay([
            *execute, tap(10.55, "result-find-toggle"),
            {"t": 11.25, "type": "paste", "text": rows[-1][1]},
        ], until=12.5, size="1440x640")
        fits(found)
        key = "result-cell-95-1"
        self.expect_rendered(found, text=rows[-1][1], within=key)
        self.expect_rendered(found, key="result-find-count", text="1 of 1")
        cell = self._settled_rect(found, key)
        viewport = self._settled_rect(found, "result-data-grid-scroll")
        header = self._settled_rect(found, "result-data-grid-header")
        self.assertGreaterEqual(cell[1] + .25, viewport[1] + header[3])
        self.assertLessEqual(cell[1] + cell[3], viewport[1] + viewport[3] + .25)
        self.assertEqual(self.db.read_bytes(), original)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())


    def test_automatic_query_result_fits_active_real_rows_unless_it_reaches_half(self):
        fixture(self.store, self.db)
        with closing(sqlite3.connect(self.db)) as db, db:
            db.executemany("INSERT INTO customers VALUES (?, ?)", [
                (i, f"auto-row-{i:02}") for i in range(3, 97)
            ])
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original = self.db.read_bytes()
        settings = self.root / "settings.json"
        settings.write_text('{"ui":{"language":"en","themeMode":"light"}}', encoding="utf-8")
        settings.chmod(0o600)
        original_settings = settings.read_bytes()

        def sql(n):
            return f"SELECT id, customer_name FROM customers WHERE id <= {n} ORDER BY id;"

        def geometry(tree, count):
            main = self._settled_rect(tree, "query-main")
            editor = self._settled_rect(tree, "query-editor-pane")
            divider = self._settled_rect(tree, "splitter:query-editor-height")
            pane = self._settled_rect(tree, "query-results-host")
            tabs = self._settled_rect(tree, "query-result-tabs")
            frame = self._settled_rect(tree, "result-data-grid-scroll-scroll-area")
            header = self._settled_rect(tree, "result-data-grid-header")
            status = self._settled_rect(tree, "content-status-bar")
            end = lambda rect: rect[1] + rect[3]
            self.assertGreaterEqual(editor[3], 120)
            self.assertEqual(divider[3], 6)
            self.assertAlmostEqual(end(editor), divider[1], delta=.25)
            self.assertAlmostEqual(end(divider), pane[1], delta=.25)
            self.assertAlmostEqual(pane[1], tabs[1], delta=.25)
            self.assertAlmostEqual(end(frame), end(pane), delta=.25)
            self.assertAlmostEqual(end(pane), end(main), delta=.25)
            self.assertAlmostEqual(end(main), status[1], delta=.25)
            chrome = pane[3] - frame[3]
            self.assertGreater(chrome, tabs[3] + 50, "count/status/controls are inside the cap")
            content = header[3] + (32 * count if count else 112)
            half = main[3] / 2
            self.assertAlmostEqual(pane[3], min(half, max(140, chrome + content)), delta=.25)
            # SQLite reports projection metadata from a returned row. The
            # same driver in the original host reports zero columns when empty.
            columns = 2 if count else 0
            self.expect_rendered(tree, key="result-grid-status", text=f"result · {count} rows × {columns} cols")
            self.assertEqual(visible(tree, key="result-grid-empty"), count == 0)
            return pane[3], half

        results = {}
        for size in ("1440x900", "1440x640"):
            heights = {}
            for count in (0, 1, 3, 96):
                with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
                    expected = db.execute(sql(count)).fetchall()
                self.assertEqual(len(expected), count)
                tree = self.replay(query_steps(sql(count)), until=10.4, size=size)
                heights[count], half = geometry(tree, count)
                if count:
                    self.expect_rendered(tree, text=expected[0][1], within="result-cell-0-1")
                elif tree["viewport"][1] == 900:
                    self.expect_rendered(tree, text="No result rows", within="result-grid-empty")
                self.assertEqual(settings.read_bytes(), original_settings, "automatic sizing is not a saved split")
            self.assertLess(heights[1], heights[3])
            self.assertLess(heights[1], heights[0])
            self.assertEqual(heights[96], half)
            results[size] = heights
        self.assertEqual(results["1440x900"][1], results["1440x640"][1])
        self.assertEqual(results["1440x900"][96] - results["1440x640"][96], 130)

        # Both statements remain in the Result tabs. The active small result
        # must reclaim space, not size from the largest/last delivered grid.
        second = [
            *query_steps(sql(1)), tap(10.5, "sql-editor-scroll"),
            {"t": 10.7, "type": "key", "name": "a", "cmd": True},
            type_text(10.9, sql(96)), tap(13.0, "sql-run"),
        ]
        switching = self.replay([*second, tap(15.5, "query-result-tab-1")], until=16.2)
        one, _ = geometry(switching, 1)
        self.assertEqual(one, results["1440x900"][1])
        self.expect_rendered(switching, key="query-result-tab-2")

        # A real off-screen Find still reveals the last row in the capped
        # pane; optional controls use part of that same pane, not extra height.
        found = self.replay([
            *query_steps(sql(96)), tap(10.55, "result-find-toggle"),
            {"t": 11.25, "type": "paste", "text": "auto-row-96"},
        ], until=12.5, size="1440x640")
        self.assertEqual(geometry(found, 96)[0], results["1440x640"][96])
        key = "result-cell-95-1"
        self.expect_rendered(found, key="result-find-count", text="1 of 1")
        self.expect_rendered(found, text="auto-row-96", within=key)
        cell = self._settled_rect(found, key)
        viewport = self._settled_rect(found, "result-data-grid-scroll")
        header = self._settled_rect(found, "result-data-grid-header")
        self.assertGreaterEqual(cell[1] + .25, viewport[1] + header[3])
        self.assertLessEqual(cell[1] + cell[3], viewport[1] + viewport[3] + .25)
        self.assertEqual(settings.read_bytes(), original_settings)
        self.assertEqual(self.db.read_bytes(), original)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())
