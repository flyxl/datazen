#!/usr/bin/env python3
"""Scoped physical Native keyboard replays for DB.status-footer commands."""

import argparse
import json
import os
from pathlib import Path
import sqlite3
import sys
import unittest

import run as native
from offline import key
from scenarios import fixture, tap, visible
from sql_formatter import draft, node, selection
from tcp_sql import DisposableSql


NEW = "test_new_query_shortcut_owns_only_the_connected_sql_context"
POSTGRES = "test_new_query_shortcut_keeps_the_active_postgres_database"
SAVE = "test_focused_save_query_opens_favorite_without_writing_until_confirmation"
FORMAT = "test_existing_format_shortcut_still_formats_focused_draft"
ERROR = "test_favorite_write_error_keeps_the_form_retryable"
CASES = (NEW, POSTGRES, SAVE, FORMAT, ERROR)
FAVORITE = "query-favorite-dialog"
FAVORITE_TITLE = "query-favorite-title-input"


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

    def readonly_sqlite(self, **settings):
        fixture(self.store, self.db)
        profiles = json.loads(self.store.read_bytes())
        profiles["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profiles), encoding="utf-8")
        self.store.chmod(0o600)
        self.db.chmod(0o400)
        return write_english_settings(self.root, **settings)

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


    def test_focused_save_query_opens_favorite_without_writing_until_confirmation(self):
        # Explicit Format conflict: a focused CodeMirror-style Save must win
        # once, leave the bytes (and selection) alone, and never run any SQL.
        settings = self.readonly_sqlite(customKeymap={"formatSql": "Mod-s"})
        inputs = (self.store.read_bytes(), self.db.read_bytes(), settings.read_bytes())
        favorites = self.root / "favorites"
        sql = "  select  1;\nSELECT '保存' AS marker;  "
        opening = [tap(.5, "conn-open-e2e-sqlite"), chord(3.5, "n")]
        blank = self.replay([*opening, chord(4.5, "s")], until=5.6)
        self.assertFalse(visible(blank, key=FAVORITE))
        self.assertFalse(list(favorites.glob("*.sql")))
        editor = [*opening, paste(4.4, sql)]
        find = self.replay([*editor, chord(5.3, "f"), tap(5.9, "sql-find-input"),
                            chord(6.4, "s")], until=7.4)
        self.expect_rendered(find, key="sql-find-input")
        field = node(find, "sql-find-input")["path"] + "/"
        self.assertTrue(any(row.get("focused") and row["path"].startswith(field)
                            for row in find["elements"]))
        self.assertFalse(visible(find, key=FAVORITE))
        self.assertEqual(draft(find), sql)
        self.assertFalse(list(favorites.glob("*.sql")))

        # Select only part of the second statement. Save still previews the
        # whole current document with outside whitespace trimmed as in React.
        choose = [*editor, key(5.3, "End"), {**key(5.55, "Home"), "shift": True}]
        before_dialog = self.replay(choose, until=5.8)
        selected = selection(before_dialog)
        self.assertTrue(selected)
        self.assertNotEqual("".join(text for _, text in selected).strip(), sql.strip())
        modal = [*choose, chord(5.9, "s"), key(6.65, "Enter"), chord(7.0, "n"), chord(7.2, "s")]
        waiting = self.replay(modal, until=8)
        self.expect_rendered(waiting, key=FAVORITE)
        self.expect_rendered(waiting, key="query-favorite-sql", text=sql.strip())
        title_path = node(waiting, FAVORITE_TITLE)["path"] + "/"
        self.assertTrue(any(row.get("focused") and row["path"].startswith(title_path)
                            for row in waiting["elements"]))
        self.assert_tabs(waiting, ["tab-1"])
        self.assertEqual(draft(waiting), sql)
        self.assertFalse(list(favorites.glob("*.sql")))

        cancelled = self.replay([*modal, paste(8.1, "do not store"), key(8.55, "Escape")], until=9.5)
        self.assertFalse(visible(cancelled, key=FAVORITE))
        self.assertTrue(node(cancelled, "sql-editor-scroll")["focused"])
        self.assertEqual(draft(cancelled), sql)
        self.assertEqual(selection(cancelled), selected)
        self.assertFalse(list(favorites.glob("*.sql")))

        title = "Keyboard saved SQL"
        confirmed = self.replay([*modal, paste(8.1, title), tap(8.7, "query-favorite-save")], until=9.8)
        self.assertFalse(visible(confirmed, key=FAVORITE))
        self.assertTrue(node(confirmed, "sql-editor-scroll")["focused"])
        self.assertEqual(draft(confirmed), sql)
        self.assertEqual(selection(confirmed), selected)
        saved = list(favorites.glob("*.sql"))
        self.assertEqual(len(saved), 1)
        content = saved[0].read_text(encoding="utf-8")
        self.assertIn(f"-- title: {title}\n", content)
        self.assertIn("-- connectionId: e2e-sqlite\n", content)
        self.assertTrue(content.endswith(sql.strip() + "\n"))

        # Fresh process through the same connected editor and the actual
        # Favorites read/open action. Do not execute the saved statements.
        restored = self.replay([*opening, tap(5.0, "query-library-favorites-toggle"),
                                tap(6.0, "query-favorite-0")], until=7.4)
        self.expect_rendered(restored, text=title, within="query-favorites-panel")
        self.assertEqual(draft(restored), sql.strip() + "\n")
        self.assertEqual((self.store.read_bytes(), self.db.read_bytes(), settings.read_bytes()), inputs)
        self.assertFalse((self.root / "query-history.json").exists())
        self.assertEqual(list(favorites.glob("*.sql")), saved)

    def test_existing_format_shortcut_still_formats_focused_draft(self):
        self.readonly_sqlite()
        tree = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), chord(3.5, "n"),
            paste(4.4, "select  1;"), chord(5.4, "f", shift=True),
        ], until=7.5)
        self.assertEqual(draft(tree), "SELECT\n  1;")
        self.assertFalse(visible(tree, key=FAVORITE))
        self.assertFalse((self.root / "query-history.json").exists())
        self.assertFalse(list((self.root / "favorites").glob("*.sql")))

    def test_favorite_write_error_keeps_the_form_retryable(self):
        self.readonly_sqlite()
        favorites = self.root / "favorites"
        favorites.mkdir()
        favorites.chmod(0o500)
        self.addCleanup(favorites.chmod, 0o700)
        if os.access(favorites, os.W_OK):
            self.skipTest("runner permissions bypass fixture's denied write")
        sql = "SELECT 'retry';"
        actions = [tap(.5, "conn-open-e2e-sqlite"), chord(3.5, "n"), paste(4.4, sql),
                   chord(5.35, "s"), paste(6.2, "Retry favorite"), key(6.85, "Enter")]
        rejected = self.replay(actions, until=7.85)
        self.expect_rendered(rejected, key="query-favorite-save-error")
        self.expect_rendered(rejected, key="input", text="Retry favorite", within=FAVORITE_TITLE)
        self.expect_rendered(rejected, key="query-favorite-sql", text=sql)
        self.assertFalse(list(favorites.glob("*.sql")))
        favorites.chmod(0o700)
        accepted = self.replay(actions, until=7.85)
        self.assertFalse(visible(accepted, key=FAVORITE))
        self.assertEqual(len(list(favorites.glob("*.sql"))), 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--postgres", type=Path)
    parser.add_argument("--case", action="append", choices=CASES)
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    if args.list_tests:
        print(*CASES, sep="\n")
        return 0
    if args.binary is None:
        parser.error("a Native executable is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    QueryShortcutJourneys.postgres = args.postgres
    cases = args.case or [NEW, SAVE]
    if not args.case and args.postgres:
        cases.append(POSTGRES)
    if POSTGRES in cases and args.postgres is None:
        parser.error("the PostgreSQL case requires --postgres pointing at a disposable fixture")
    suite = unittest.TestSuite(QueryShortcutJourneys(name) for name in cases)
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
