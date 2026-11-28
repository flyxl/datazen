"""Physical typed asymmetric SQL brackets on the actual Native Query input surface."""
import json

from offline import key
from scenarios import fixture, tap, visible


class SqlBracketPairJourney:
    def test_native_query_typed_parentheses_wrap_and_skip_generated_only(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text())
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        db_before = self.db.read_bytes()

        editor = "sql-editor-scroll"
        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"), tap(4.0, editor)]

        def paren(at, text):
            return {"t": at, "type": "key", "name": text,
                    "code": "9" if text == "(" else "0", "shift": True}

        def paste(at, text):
            return {"t": at, "type": "paste", "text": text}

        def draft(tree, text):
            line = next(node for node in tree["elements"]
                        if node.get("key") == "sql-line-0" and not node.get("exiting"))
            actual = "".join(node.get("text") or "" for node in tree["elements"]
                             if node["path"].startswith(line["path"] + "/") and not node.get("exiting"))
            self.assertEqual(actual, text)
            self.assertFalse(visible(tree, key="result-workspace"))

        paired = [*route, paste(4.5, "SELECT "), paren(5.05, "(")]
        draft(self.replay(paired, until=5.65), "SELECT ()")
        typed_inside = [*paired, {"t": 5.7, "type": "key", "name": "4", "code": "4"}]
        passed = [*typed_inside, paren(6.2, ")")]
        draft(self.replay(passed, until=6.8), "SELECT (4)")
        draft(self.replay([*passed, paren(7.0, ")")], until=7.6), "SELECT (4))")
        draft(self.replay([*passed, key(7.0, "z", cmd=True)], until=7.6), "SELECT ()")

        selected = "SELECT '中文';"
        wrapping = [*route, paste(4.5, selected), key(5.1, "a", cmd=True), paren(5.6, "(")]
        wrapped = self.replay(wrapping, until=6.2)
        draft(wrapped, f"({selected})")
        self.expect_rendered(wrapped, key="sql-selected-0-1", text=selected)
        undone = self.replay([*wrapping, key(6.35, "z", cmd=True)], until=6.95)
        draft(undone, selected)
        self.expect_rendered(undone, key="sql-selected-0-0", text=selected)

        manual = [*route, paste(4.5, "SELECT ()"), key(5.0, "Left"), paren(5.55, ")")]
        draft(self.replay(manual, until=6.15), "SELECT ())")
        draft(self.replay([*route, paste(4.5, "(")], until=5.1), "(")

        no_pair_before_word = [*route, paste(4.5, "word"), key(5.05, "Home"), paren(5.55, "(")]
        draft(self.replay(no_pair_before_word, until=6.15), "(word")
        self.assertEqual(self.db.read_bytes(), db_before)
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text()))

    def test_native_query_typed_square_and_curly_brackets_keep_provenance_and_focus(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        before = self.db.read_bytes()

        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"),
                 tap(4.0, "sql-editor-scroll")]

        def press(at, glyph):
            left = glyph in "[{"
            return {"t": at, "type": "key", "name": glyph,
                    "code": "BracketLeft" if left else "BracketRight",
                    "base": "[" if left else "]", "shift": glyph in "{}"}

        def paste(at, content):
            return {"t": at, "type": "paste", "text": content}

        def draft(tree, expected):
            line = next(n for n in tree["elements"]
                        if n.get("key") == "sql-line-0" and not n.get("exiting"))
            actual = "".join(n.get("text") or "" for n in tree["elements"]
                             if n["path"].startswith(line["path"] + "/") and not n.get("exiting"))
            self.assertEqual(actual, expected)
            self.assertFalse(visible(tree, key="result-workspace"))

        nested = [*route, paste(4.5, "SELECT "), press(5.0, "["), press(5.5, "{")]
        draft(self.replay(nested, until=6.0), "SELECT [{}]")
        wrong_kind = [*nested, press(6.15, "]")]
        draft(self.replay(wrong_kind, until=6.65), "SELECT [{]}]")
        passed = [*wrong_kind, press(6.75, "}"), press(7.15, "]")]
        draft(self.replay(passed, until=7.65), "SELECT [{]}]")
        draft(self.replay([*passed, key(7.8, "z", cmd=True)], until=8.4), "SELECT [{}]")

        original = "SELECT '中文';"
        wrapped = [*route, paste(4.5, original), key(5.1, "a", cmd=True), press(5.65, "{")]
        tree = self.replay(wrapped, until=6.2)
        draft(tree, "{" + original + "}")
        self.expect_rendered(tree, key="sql-selected-0-1", text=original)
        restored = self.replay([*wrapped, key(6.45, "z", cmd=True)], until=7.0)
        draft(restored, original)
        self.expect_rendered(restored, key="sql-selected-0-0", text=original)

        pasted = [*route, paste(4.5, "SELECT []"), key(5.05, "Left"), press(5.6, "]")]
        draft(self.replay(pasted, until=6.2), "SELECT []]")
        found = self.replay([*route, paste(4.5, original), key(5.1, "f", cmd=True),
                             press(5.75, "["), press(6.1, "{")], until=6.85)
        self.expect_rendered(found, text="[{", within="sql-find-input")
        draft(found, original)

        self.assertEqual(self.db.read_bytes(), before)
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
