#!/usr/bin/env python3
"""Opt-in SQLite DDL header/copy regression in the normal Native workspace."""
import argparse
import json
import sqlite3
import unittest
from pathlib import Path

import run as native
from scenarios import fixture, tap


class TableDdlJourney(native.NativeJourneys):
    def test_readonly_table_ddl_header_copies_actual_selected_definition(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text())
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile))
        (self.root / "settings.json").write_text(
            json.dumps({"ui": {"language": "en", "themeMode": "light"}})
        )
        with sqlite3.connect(self.db) as conn:
            conn.execute("CREATE TABLE ledger (\n  label TEXT DEFAULT 'do not replace',\n  cents INTEGER\n)")
            ddl = conn.execute("SELECT sql FROM sqlite_schema WHERE name='ledger'").fetchone()[0]
        original = self.db.read_bytes()
        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-ledger"), tap(5, "view-ddl")]
        for size in ("1440x900", "960x640"):
            tree = self.replay(route, until=6.8, size=size)
            for k, label in (("table-ddl-title", "ledger"), ("table-ddl-kind", "· DDL")):
                self.expect_rendered(tree, key=k, text=label, within="table-ddl-header")
            self.expect_rendered(tree, text="Copy", within="table-ddl-copy")
            for line in ddl.splitlines():
                self.expect_rendered(tree, text=line, within="table-ddl")
            x, y, width, h = self._settled_rect(tree, "table-ddl-header")
            lx, _, _, _ = self._settled_rect(tree, "table-ddl-title")
            cx, cy, cw, ch = self._settled_rect(tree, "table-ddl-copy")
            _, code_y, _, _ = self._settled_rect(tree, "table-ddl")
            self.assertAlmostEqual(lx-x, 16, delta=1.0)
            self.assertAlmostEqual(x+width-cx-cw, 16, delta=1.0)
            self.assertGreaterEqual(cy, y)
            self.assertLessEqual(cy+ch, y+h+1)
            self.assertAlmostEqual(code_y, y+h, delta=1.0)
        copied = [*route, tap(7, "table-ddl-copy")]
        feedback = self.replay(copied, until=7.6)
        self.expect_rendered(feedback, text="Copied", within="table-ddl-copy")
        restored = self.replay(copied, until=9.8)
        self.expect_rendered(restored, text="Copy", within="table-ddl-copy")
        self.assertEqual(self.db.read_bytes(), original)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    name = "test_readonly_table_ddl_header_copies_actual_selected_definition"
    if args.list_tests:
        print(name)
        return 0
    if not args.binary:
        parser.error("a Native executable is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    suite = unittest.TestSuite([TableDdlJourney(name)])
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
