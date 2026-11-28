"""CodeMirror's base SQL line comment shortcut in the real Native Query canvas."""
import json

from offline import key
from scenarios import fixture, tap, visible


class SqlLineCommentJourney:
    def test_native_sql_line_comment_preserves_ranges_and_undo_without_query_execution(self):
        fixture(self.store, self.db)
        connections = json.loads(self.store.read_text())
        connections["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(connections), encoding="utf-8")
        before = self.db.read_bytes()
        source = ["SELECT 'other first';", "  SELECT '中文';", "SELECT 'other last';"]
        original = "\n".join(source)
        commented = [source[0], "  -- SELECT '中文';", source[2]]

        def line_text(tree, index):
            line = next(node for node in tree["elements"]
                        if node.get("key") == f"sql-line-{index}" and not node.get("exiting"))
            return "".join(node.get("text") or "" for node in tree["elements"]
                           if node["path"].startswith(line["path"] + "/") and not node.get("exiting"))

        def draft(tree, lines):
            self.assertEqual([line_text(tree, i) for i in range(len(lines))], lines)
            self.assertFalse(visible(tree, key="result-workspace"))

        def slash(at, *, repeat=False):
            return {"t": at, "type": "key", "name": "/", "code": "/", "cmd": True,
                    "repeat": repeat}

        selected = [
            tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"),
            tap(4.0, "sql-editor-scroll"), {"t": 4.5, "type": "paste", "text": original},
            key(5.35, "Home"), key(5.8, "Up"), key(6.25, "End"),
            {"t": 6.75, "type": "key", "name": "Home", "shift": True},
        ]
        comment = [*selected, slash(7.25), slash(7.45, repeat=True)]
        transformed = self.replay(comment, until=8.1)
        draft(transformed, commented)
        self.expect_rendered(transformed, key="sql-selected-1-0", text=commented[1])
        self.assertFalse(visible(transformed, key="sql-selected-0-0"))
        self.assertFalse(visible(transformed, key="sql-selected-2-0"))
        restored = self.replay([*comment, key(8.4, "z", cmd=True)], until=9.15)
        draft(restored, source)
        self.expect_rendered(restored, key="sql-selected-1-0", text=source[1])
        draft(self.replay([*comment, slash(8.4)], until=9.15), source)

        # An open Find text field owns its keyboard, not the Query document.
        find = self.replay([*selected, tap(7.2, "sql-find-open"), slash(8.1)], until=8.7)
        self.expect_rendered(find, key="sql-find-bar")
        draft(find, source)

        # The shortcut changes only the currently mounted draft.
        other = ["SELECT 'separate tab';"]
        switched = [
            *comment, tap(8.35, "conn-toolbar-new-query"), tap(9.15, "sql-editor-scroll"),
            {"t": 9.6, "type": "paste", "text": other[0]},
        ]
        second = self.replay(switched, until=10.2)
        draft(second, other)
        back = self.replay([*switched, tap(10.4, "tab-1")], until=11.2)
        draft(back, commented)
        self.assertEqual(self.db.read_bytes(), before)
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text()))
