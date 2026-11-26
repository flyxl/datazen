#!/usr/bin/env python3
"""Replay user input in the real DataZen native executable, with real SQLite files."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest

from csv_import import CsvImportJourneys
from dashboard import DashboardJourney
from data_sync import DataSyncJourneys
from favorites import FavoritesJourney
from offline import OfflineJourneys
from query_history import QueryHistoryJourney
from scenarios import fixture, query_steps, tap, type_text, visible
from schema_diff import SchemaDiffJourney
from sql_selection import SqlSelectionJourneys
from tcp_metadata import TcpMetadataJourneys, with_metadata_journeys
from tcp_sql import TcpSqlJourneys, with_tcp_journeys
from tcp_transfer import TcpTransferJourneys, with_sql_transfer_journeys
from transfer import TransferJourneys
from workflow import WorkflowJourney


class NativeJourneys(DashboardJourney, FavoritesJourney, OfflineJourneys, QueryHistoryJourney, WorkflowJourney, TcpSqlJourneys, TcpMetadataJourneys, TcpTransferJourneys, TransferJourneys, SchemaDiffJourney, DataSyncJourneys, CsvImportJourneys, SqlSelectionJourneys, unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(prefix="datazen-native-e2e-")
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.store = self.root / "connections.json"
        self.db = self.root / "customers.sqlite"
        self.replays = 0
        self.env = {
            **os.environ,
            "DATAZEN_NATIVE_KEYRING": "file",
            "DATAZEN_NATIVE_CONNECTIONS": str(self.store),
            "XDG_DATA_HOME": str(self.root / "data"),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_CACHE_HOME": str(self.root / "cache"),
        }
        self.env.pop("DATAZEN_NATIVE_KEYRING_PATH", None)
        self.env["XDG_RUNTIME_DIR"] = str(self.root / "runtime")
        (self.root / "runtime").mkdir(mode=0o700)

    def write_tcp_profiles(self, profiles):
        # Provision passwords through the native store CLI, never as legacy
        # untagged tokens that could be mistaken for old encrypted secrets.
        rows = [profile.copy() for profile in profiles]
        passwords = {row["id"]: row.pop("password") for row in rows}
        fd = os.open(self.store, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode=0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as store:
            json.dump({"connections": rows}, store)
        result = subprocess.run(
            [str(BINARY), "--replace-saved-passwords"],
            input=json.dumps(passwords), text=True, capture_output=True,
            env=self.env, cwd=self.root, timeout=15,
        )
        self.assertEqual(result.returncode, 0, "Native fixture credential setup failed")

    def replay(self, actions, *, until=1, exit_code=0):
        self.replays += 1
        script, tree = self.root / "input.json", self.root / "tree.json"
        tree.unlink(missing_ok=True)
        script.write_text(json.dumps(actions), encoding="utf-8")
        result = subprocess.run(
            [str(BINARY), "--script", str(script), "--tree", str(tree),
             "--size", "1440x900", "--scale", "1", "--realtime", "--at", str(until)],
            env=self.env, cwd=self.root, capture_output=True, text=True, timeout=75,
        )
        self.assertEqual(result.returncode, exit_code, result.stdout + result.stderr)
        if exit_code != 0:
            self.assertFalse(tree.exists(), "startup failure must happen before rendering")
            return result.stderr
        self.assertTrue(tree.is_file(), result.stdout + result.stderr)
        parsed = json.loads(tree.read_text(encoding="utf-8"))
        if ARTIFACTS:
            (ARTIFACTS / f"{self._testMethodName}-{self.replays:02d}.tree.json").write_text(
                json.dumps(parsed, ensure_ascii=False, indent=2), encoding="utf-8"
            )
        return parsed

    def expect_rendered(self, tree, *, key=None, text=None, within=None):
        self.assertTrue(
            visible(tree, key=key, text=text, within=within),
            f"No rendered node for key={key!r}, text={text!r}, within={within!r}",
        )

    def test_fresh_install_shows_empty_connections(self):
        tree = self.replay([])
        self.expect_rendered(tree, key="connections-empty")
        self.assertFalse(self.store.exists())

    def test_corrupt_store_is_refused_without_overwriting_user_data(self):
        corrupt = b'{"connections":'
        self.store.write_bytes(corrupt)
        self.assertIn("the connection store did not open", self.replay([], exit_code=1))
        self.assertEqual(self.store.read_bytes(), corrupt)

    def test_create_connection_is_saved_and_reappears_after_relaunch(self):
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE customers (id INTEGER PRIMARY KEY)")
        name = "E2E saved connection"
        tree = self.replay([
            tap(.5, "connections-create-first"),
            tap(1.5, "connection-name-input"),
            type_text(1.8, name),
            tap(3, "connection-engine-option-sqlite"),
            tap(3.5, "connection-database-input"),
            type_text(3.8, str(self.db)),
            tap(6.5, "connection-address-input"),
            type_text(6.8, "localhost"),
            tap(7.5, "connection-save"),
        ], until=9)
        rows = json.loads(self.store.read_text(encoding="utf-8"))["connections"]
        self.assertEqual([(row["name"], row["databaseType"], row["database"]) for row in rows],
                         [(name, "sqlite", str(self.db))])
        self.assertFalse(visible(tree, key="connection-form-error"))
        self.expect_rendered(self.replay([]), text=name)

    def test_connection_browse_reads_sqlite_rows(self):
        fixture(self.store, self.db)
        tree = self.replay([tap(0.5, "conn-open-e2e-sqlite"),
                            tap(3, "tree-table-customers")], until=5)
        self.expect_rendered(tree, key="tree-table-customers")
        self.expect_rendered(tree, text="fixture-ada")
        self.expect_rendered(tree, text="fixture-lin")

    def test_connection_failure_keeps_connection_picker_open(self):
        fixture(self.store, self.db)
        self.db.write_bytes(b"not a SQLite database")
        tree = self.replay([tap(.5, "conn-open-e2e-sqlite")], until=4)
        self.expect_rendered(tree, key="desktop-adapter-message")
        self.expect_rendered(tree, key="conn-open-e2e-sqlite")
        self.assertFalse(visible(tree, key="tree-table-customers"))
        self.assertEqual(self.db.read_bytes(), b"not a SQLite database")

    def test_table_ddl_matches_sqlite_catalog(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT sql FROM sqlite_schema WHERE name = 'customers'").fetchone()[0]
        tree = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-customers"), tap(5, "view-ddl"),
        ], until=7)
        self.expect_rendered(tree, text=expected, within="table-ddl")

    def test_table_edit_commit_and_rollback(self):
        fixture(self.store, self.db)

        def edit(at, value):
            return [
                tap(at, "cell-0-1"),
                {"t": at + .35, "type": "key", "name": "Enter"},
                tap(at + .6, "grid-editor"),
                {"t": at + .95, "type": "key", "name": "a", "cmd": True},
                {"t": at + 1.3, "type": "type", "text": value},
                {"t": at + 2.15, "type": "key", "name": "Enter"},
            ]
        browse = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-customers")]
        staging = [*browse, *edit(5, "fixture-rolled-back")]
        staged = self.replay(staging, until=7.7)
        self.expect_rendered(staged, text="fixture-rolled-back", within="cell-0-1")
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT customer_name FROM customers WHERE id = 1").fetchone(),
                             ("fixture-ada",))
        tree = self.replay([*staging, tap(7.8, "rollback-staged")], until=9)
        self.expect_rendered(tree, key="cell-0-1")
        self.expect_rendered(tree, text="fixture-ada", within="cell-0-1")
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT customer_name FROM customers WHERE id = 1").fetchone(),
                             ("fixture-ada",))
        tree = self.replay([
            *browse, *edit(5, "fixture-edited"), tap(7.8, "commit-staged"),
        ], until=11)
        self.expect_rendered(tree, text="fixture-edited", within="cell-0-1")
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT customer_name FROM customers WHERE id = 1").fetchone(),
                             ("fixture-edited",))

    def test_sql_query_result_matches_independent_database_read(self):
        fixture(self.store, self.db)
        sql = "SELECT customer_name FROM customers WHERE id = 2;"
        with sqlite3.connect(self.db) as db:
            expected = db.execute(sql).fetchone()[0]
        tree = self.replay(query_steps(sql), until=11)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.assertFalse(visible(tree, text="fixture-ada", within="result-workspace"))

    def test_sql_write_persists_and_is_read_on_relaunch(self):
        fixture(self.store, self.db)
        self.replay(query_steps("UPDATE customers SET customer_name = 'fixture-updated' WHERE id = 1;"), until=11)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT customer_name FROM customers WHERE id = 1").fetchone(),
                             ("fixture-updated",))
        tree = self.replay([tap(0.5, "conn-open-e2e-sqlite"),
                            tap(3, "tree-table-customers")], until=5)
        self.expect_rendered(tree, text="fixture-updated")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--status-json", type=Path, help="write exact unittest result counts")
    parser.add_argument("--list-tests", action="store_true")
    parser.add_argument("--postgres-config", type=Path)
    parser.add_argument("--mysql-config", type=Path)
    parsed, remaining = parser.parse_known_args()
    configs = {
        engine: config.resolve(strict=True)
        for engine, config in (("postgresql", parsed.postgres_config), ("mysql", parsed.mysql_config))
        if config is not None
    }
    with_tcp_journeys(NativeJourneys, configs)
    with_metadata_journeys(NativeJourneys, configs)
    with_sql_transfer_journeys(NativeJourneys, configs)
    if parsed.list_tests:
        if remaining:
            parser.error("--list-tests does not take test selectors")
        names = unittest.defaultTestLoader.getTestCaseNames(NativeJourneys)
        print(json.dumps([f"NativeJourneys.{name}" for name in names]))
        raise SystemExit(0)
    BINARY = parsed.binary.resolve(strict=True)
    ARTIFACTS = parsed.artifacts
    if ARTIFACTS:
        ARTIFACTS = ARTIFACTS.resolve()
        ARTIFACTS.mkdir(parents=True, exist_ok=True)
    run = unittest.main(argv=[__file__, *remaining], verbosity=2, exit=False).result
    failures = len(run.failures) + len(run.errors) + len(run.unexpectedSuccesses)
    skipped = len(run.skipped) + len(run.expectedFailures)
    if parsed.status_json:
        parsed.status_json.write_text(json.dumps({
            "run": run.testsRun, "passed": run.testsRun - failures - skipped,
            "failed": failures, "skipped": skipped,
        }) + "\n", encoding="utf-8")
    raise SystemExit(0 if run.wasSuccessful() else 1)
