#!/usr/bin/env python3
"""Scoped physical Native keyboard replays for DB.status-footer commands."""

import argparse
import json
from pathlib import Path
import sqlite3
import sys
import unittest

import run as native
from offline import key
from scenarios import fixture, tap, visible
from sql_formatter import draft, node
from tcp_sql import DisposableSql


NEW = "test_new_query_shortcut_owns_only_the_connected_sql_context"
POSTGRES = "test_new_query_shortcut_keeps_the_active_postgres_database"


def chord(at, letter, **options):
    return {**key(at, letter, cmd=True), "code": f"Key{letter.upper()}", "base": letter, **options}


def paste(at, text):
    return {"t": at, "type": "paste", "text": text}


def write_english_settings(root, **extra):
    path = root / "settings.json"
    path.write_text(json.dumps({"ui": {"language": "en", "themeMode": "light"}, **extra}), encoding="utf-8")
    return path


class QueryShortcutJourneys(native.NativeJourneys):
    postgres = None

    def readonly_sqlite(self):
        fixture(self.store, self.db)
        profiles = json.loads(self.store.read_bytes())
        profiles["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profiles), encoding="utf-8")
        self.store.chmod(0o600)
        self.db.chmod(0o400)
        return write_english_settings(self.root)

    def assert_tabs(self, tree, expected):
        self.assertEqual(
            sorted(element["key"] for element in tree["elements"]
                   if (element.get("key") or "").removeprefix("tab-").isdigit()
                   and (element.get("key") or "").startswith("tab-") and not element.get("exiting")),
            expected,
        )

    def test_new_query_shortcut_owns_only_the_connected_sql_context(self):
        settings = self.readonly_sqlite()
        files_before = (self.store.read_bytes(), self.db.read_bytes(), settings.read_bytes())
        landing = self.replay([chord(1.1, "n")], until=2.4)
        self.assert_tabs(landing, [])
        self.assertEqual(visible(landing, key="connection-dialog"), sys.platform in ("linux", "win32"))

        opened = [tap(.5, "conn-open-e2e-sqlite")]
        sql = "SELECT customer_name AS bound_db_row FROM customers WHERE id=2;"
        with sqlite3.connect(self.db.as_uri() + "?mode=ro", uri=True) as database:
            expected = database.execute(sql).fetchone()[0]
        routed = [*opened, chord(3.5, "n"), paste(4.4, sql)]
        first = self.replay(routed, until=5.7)
        self.assert_tabs(first, ["tab-1"])
        self.assertEqual(draft(first), sql)
        self.assertTrue(node(first, "sql-editor-scroll")["focused"])
        self.assertFalse(visible(first, key="connection-dialog"))
        self.assertFalse((self.root / "query-history.json").exists())

        followup = [*routed, chord(5.8, "n")]
        fresh = self.replay(followup, until=7.1)
        self.assert_tabs(fresh, ["tab-1", "tab-2"])
        self.assertEqual(draft(fresh), "")
        self.assertTrue(node(fresh, "sql-editor-scroll")["focused"])
        result = self.replay([*followup, tap(7.3, "tab-1"), tap(8.2, "sql-run")], until=11.2)
        self.expect_rendered(result, text=expected, within="result-workspace")
        self.assertFalse(visible(result, key="connection-dialog"))

        if sys.platform in ("linux", "win32"):
            menu = [*followup, tap(7.3, "menu-bar-file-trigger")]
            open_menu = self.replay(menu, until=8.1)
            self.expect_rendered(open_menu, text="New Connection", within="menu-bar-file-item0")
            self.assertFalse(visible(open_menu, text="Ctrl+N", within="menu-bar-file-item0"))
            explicit = [*menu, tap(8.2, "menu-bar-file-item0"), chord(9.7, "n")]
            blocked = self.replay(explicit, until=11.2)
            self.expect_rendered(blocked, key="connection-dialog")
            self.assert_tabs(blocked, ["tab-1", "tab-2"])
        self.assertEqual((self.store.read_bytes(), self.db.read_bytes(), settings.read_bytes()), files_before)
        self.assertFalse(list((self.root / "favorites").glob("*.sql")))

    def test_new_query_shortcut_keeps_the_active_postgres_database(self):
        if self.postgres is None:
            self.skipTest("explicit disposable loopback --postgres fixture required")
        primary = DisposableSql("postgresql", self.postgres)
        alternate = DisposableSql("postgresql", self.postgres, alternate=True)
        primary.write_profile(self, read_only=True, default_database=False)
        write_english_settings(self.root)
        ordered = primary.query("SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;").splitlines()
        self.assertEqual(ordered[0], primary.database)
        selector = "query-context-database-trigger"
        sql = "SELECT current_database() AS keyboard_query_database;"
        tree = self.replay([
            tap(.5, "conn-open-e2e-tcp-sql"),
            chord(5.8, "n"),
            tap(7.0, selector),
            tap(8.2, f"option{ordered.index(alternate.database)}"),
            tap(9.2, "sql-editor-scroll"),
            chord(9.8, "n"),
            paste(10.6, sql),
            tap(11.8, "sql-run"),
        ], until=15.5)
        self.assert_tabs(tree, ["tab-1", "tab-2"])
        readback = alternate.query(sql)
        self.expect_rendered(tree, text=readback, within=selector)
        self.expect_rendered(tree, text=readback, within="result-cell-0-0")
        history = json.loads((self.root / "query-history.json").read_bytes())
        self.assertEqual([(entry["database"], entry["success"]) for entry in history], [(readback, True)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--postgres", type=Path)
    parser.add_argument("--case", action="append", choices=(NEW, POSTGRES))
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    if args.list_tests:
        print(NEW, POSTGRES, sep="\n")
        return 0
    if args.binary is None:
        parser.error("a Native executable is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    QueryShortcutJourneys.postgres = args.postgres
    cases = args.case or ([NEW, POSTGRES] if args.postgres else [NEW])
    if POSTGRES in cases and args.postgres is None:
        parser.error("the PostgreSQL case requires --postgres pointing at a disposable fixture")
    suite = unittest.TestSuite(QueryShortcutJourneys(name) for name in cases)
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
