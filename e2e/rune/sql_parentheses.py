"""Physical typed round parentheses on the actual Native Query input surface."""
import json

from offline import key
from scenarios import fixture, tap, visible


class SqlRoundPairJourney:
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
