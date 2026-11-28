"""Scoped Go to Line input, live native SQLite editor and centered caret."""
import json

from offline import key
from scenarios import fixture, tap, visible


def _node(tree, name):
    return next(x for x in tree["elements"] if x.get("key") == name and not x.get("exiting"))


def _painted_sql_row(tree, name):
    # Rune splits a line around the caret. Read only visible text in this row
    # (the fixture is left-to-right ASCII), not the caret or offscreen copies.
    row = _node(tree, name)
    rx, ry, rw, rh = row["rect"]
    vx, vy, vw, vh = _node(tree, "sql-editor-scroll")["rect"]
    win_w, win_h = tree["viewport"]
    left, top = max(0, rx, vx), max(0, ry, vy)
    right, bottom = min(win_w, rx + rw, vx + vw), min(win_h, ry + rh, vy + vh)
    prefix = row["path"] + "/"
    fragments = []
    for fragment in tree["elements"]:
        if (fragment.get("exiting") or fragment["opacity"] <= 0
                or not fragment["path"].startswith(prefix) or fragment["role"] != "text"
                or not fragment.get("text")):
            continue
        x, y, w, h = fragment["rect"]
        if max(x, left) < min(x + w, right) and max(y, top) < min(y + h, bottom):
            fragments.append((x, fragment["text"]))
    fragments.sort(key=lambda fragment: fragment[0])
    return "".join(text for _, text in fragments)


class SqlGoToLineJourney:
    def test_native_query_goto_line_center_focus_and_no_sql_execution(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        before = self.db.read_bytes()
        sql = "\n".join(f"-- row {n:03d}" for n in range(1, 121))
        def paste(at, text):
            return {"t": at, "type": "paste", "text": text}
        def go(at):
            return {"t": at, "type": "key", "name": "g", "code": "KeyG",
                    "base": "g", "cmd": True, "alt": True}
        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"),
                 tap(4.0, "sql-editor-scroll"), paste(4.5, sql)]
        opened = [*route, go(5.0)]
        dialog = self.replay(opened, until=5.5)
        self.expect_rendered(dialog, text="120", within="sql-goto-line-input")
        submitted = [*opened, paste(5.6, "60:3"), key(6.1, "Enter")]
        jumped = self.replay(submitted, until=6.6)
        self.assertFalse(visible(jumped, key="sql-goto-line-panel"))
        editor = _node(jumped, "sql-editor-scroll")
        self.assertTrue(editor.get("focused"))
        _, top, _, height = editor["rect"]
        caret = _node(jumped, "sql-caret")
        _, y, _, h = caret["rect"]
        self.assertAlmostEqual(y + h / 2, top + height / 2, delta=3)
        self.assertTrue(caret["path"].startswith(_node(jumped, "sql-line-59")["path"] + "/"))
        marked = self.replay([*submitted, paste(6.8, "X")], until=7.4)
        self.assertEqual(_painted_sql_row(marked, "sql-line-59"), "-- Xrow 060")
        cancelled = self.replay([*opened, paste(5.6, "60"), key(5.9, "Escape"),
                                 paste(6.3, "X")], until=6.9)
        self.assertEqual(_painted_sql_row(cancelled, "sql-line-119"), "-- row 120X")
        find = self.replay([*route, key(4.9, "f", cmd=True), go(5.4)], until=5.9)
        self.assertTrue(visible(find, key="sql-find-input"))
        self.assertFalse(visible(find, key="sql-goto-line-panel"))
        for scene in (dialog, jumped, marked, cancelled, find):
            self.assertFalse(visible(scene, key="result-workspace"))
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_bytes()))
        self.assertEqual(self.db.read_bytes(), before)
