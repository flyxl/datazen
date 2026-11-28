"""Stock native query/result divider, retained SQLite rows and real geometry."""
import json
import sqlite3
from contextlib import closing

from scenarios import fixture, query_steps, tap


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
        previous_editor, _, previous_frame, _ = sizes(before)
        self.assertEqual(previous_frame[3], 200, "not rebudgeted until the user pins the split")
        self.assertIsNone(saved_height())

        tall = self.replay([*execute, drag(-80)], until=12.0)
        editor, frame = fits(tall)
        self.assertAlmostEqual(editor[3], previous_editor[3] - 80, delta=1)
        self.assertGreater(frame[3], previous_frame[3])
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
