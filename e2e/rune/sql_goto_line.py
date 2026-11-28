"""Scoped Go to Line input, live native SQLite editor and centered caret."""
from offline import key
from scenarios import fixture, tap, visible


class SqlGoToLineJourney:
    def test_native_query_goto_line_center_focus_and_no_sql_execution(self):
        fixture(self.store, self.db)
        before = self.db.read_bytes()
        sql = "\n".join(f"-- row {n:03d}" for n in range(1, 121))
        def paste(at, text):
            return {"t": at, "type": "paste", "text": text}
        def go(at):
            return {"t": at, "type": "key", "name": "g", "code": "KeyG",
                    "base": "g", "cmd": True, "alt": True}
        def node(tree, name):
            return next(x for x in tree["elements"] if x.get("key") == name and not x.get("exiting"))
        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"),
                 tap(4.0, "sql-editor-scroll"), paste(4.5, sql)]
        opened = [*route, go(5.0)]
        dialog = self.replay(opened, until=5.5)
        self.expect_rendered(dialog, text="120", within="sql-goto-line-input")
        submitted = [*opened, paste(5.6, "60:3"), key(6.1, "Enter")]
        jumped = self.replay(submitted, until=6.6)
        self.assertFalse(visible(jumped, key="sql-goto-line-panel"))
        _, top, _, height = node(jumped, "sql-editor-scroll")["rect"]
        caret = node(jumped, "sql-caret")
        _, y, _, h = caret["rect"]
        self.assertAlmostEqual(y + h / 2, top + height / 2, delta=3)
        self.assertTrue(caret["path"].startswith(node(jumped, "sql-line-59")["path"] + "/"))
        marked = self.replay([*submitted, paste(6.8, "X")], until=7.4)
        self.expect_rendered(marked, text="-- Xrow 060", within="sql-line-59")
        cancelled = self.replay([*opened, paste(5.6, "60"), key(5.9, "Escape"),
                                 paste(6.3, "X")], until=6.9)
        self.expect_rendered(cancelled, text="-- row 120X", within="sql-line-119")
        find = self.replay([*route, key(4.9, "f", cmd=True), go(5.4)], until=5.9)
        self.assertTrue(visible(find, key="sql-find-input"))
        self.assertFalse(visible(find, key="sql-goto-line-panel"))
        self.assertFalse(visible(jumped, key="result-workspace"))
        self.assertEqual(self.db.read_bytes(), before)
