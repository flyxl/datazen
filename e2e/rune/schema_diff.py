"""Read-only native column comparison against two actual SQLite catalogs."""
from contextlib import closing
import json
import sqlite3

from scenarios import tap, visible


class SchemaDiffJourney:
    def test_sqlite_schema_diff_reports_real_columns_without_writing_either_file(self):
        target = self.root / "schema-target.sqlite"
        alias = self.root / "schema-alias.sqlite"
        with closing(sqlite3.connect(self.db)) as source:
            source.executescript("""
                CREATE TABLE "a.desired" (id INTEGER PRIMARY KEY, amount INTEGER, source_note TEXT);
                INSERT INTO "a.desired" VALUES (1, 42, 'source fixture');
                CREATE TABLE z_equal (id INTEGER PRIMARY KEY, label TEXT);
                INSERT INTO z_equal VALUES (2, 'source matched schema');
            """)
        with closing(sqlite3.connect(target)) as dest:
            dest.executescript("""
                CREATE TABLE "a.current" (id INTEGER PRIMARY KEY, amount TEXT, target_note TEXT);
                INSERT INTO "a.current" VALUES (11, 'keep', 'target fixture');
                CREATE TABLE z_equal (id INTEGER PRIMARY KEY, label TEXT);
                INSERT INTO z_equal VALUES (3, 'target matched schema');
            """)
        alias.hardlink_to(self.db)

        def snapshot(path, table):
            # A separate SQLite connection is the oracle, not application DTOs
            # or the rendered tree. Capture real column flags, rows and journals.
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                return (db.execute("SELECT name, type, [notnull], pk FROM pragma_table_info(?)",
                                   (table,)).fetchall(),
                        db.execute(f'SELECT * FROM "{table}" ORDER BY id').fetchall(),
                        db.execute("PRAGMA journal_mode").fetchone())

        pairs = [(self.db, "a.desired"), (target, "a.current"),
                 (self.db, "z_equal"), (target, "z_equal")]
        baseline = [snapshot(path, table) for path, table in pairs]
        self.assertEqual(baseline, [
            ([("id", "INTEGER", 0, 1), ("amount", "INTEGER", 0, 0),
              ("source_note", "TEXT", 0, 0)], [(1, 42, "source fixture")], ("delete",)),
            ([("id", "INTEGER", 0, 1), ("amount", "TEXT", 0, 0),
              ("target_note", "TEXT", 0, 0)], [(11, "keep", "target fixture")], ("delete",)),
            ([("id", "INTEGER", 0, 1), ("label", "TEXT", 0, 0)],
             [(2, "source matched schema")], ("delete",)),
            ([("id", "INTEGER", 0, 1), ("label", "TEXT", 0, 0)],
             [(3, "target matched schema")], ("delete",)),
        ])
        originals = [path.read_bytes() for path in (self.db, target)]

        def expect_unchanged():
            self.assertEqual([snapshot(path, table) for path, table in pairs], baseline)
            self.assertEqual([path.read_bytes() for path in (self.db, target)], originals)
            for path in (self.db, target, alias):
                for suffix in ("-wal", "-shm", "-journal"):
                    self.assertFalse(path.with_name(path.name + suffix).exists())

        def profile(identifier, name, path, read_only):
            return {"id": identifier, "name": name, "databaseType": "sqlite",
                    "database": str(path), "readOnly": read_only, "connectionTimeout": 8}

        self.store.write_text(json.dumps({"connections": [
            profile("schema-src", "Schema Source", self.db, False),
            profile("schema-dst", "Schema Target", target, True),
            profile("schema-alias", "Schema Source Alias", alias, False),
        ]}), encoding="utf-8")
        setup = [tap(.5, "rail-tools"), tap(1.2, "native-tool-schema-diff"),
                 tap(2.0, "native-transfer-source-profile-schema-src")]
        alias_steps = [*setup, tap(2.6, "native-transfer-target-profile-schema-alias"),
                       tap(4.0, "native-schema-compare")]
        rejected = self.replay(alias_steps, until=5.3)
        self.expect_rendered(rejected, key="native-schema-error",
                             text="Select two different SQLite files.")
        self.assertFalse(visible(rejected, key="native-schema-results"))
        expect_unchanged()

        diff_steps = [*setup, tap(2.6, "native-transfer-target-profile-schema-dst"),
                      tap(4.1, "native-schema-compare")]
        rendered = self.replay(diff_steps, until=6.0)
        self.expect_rendered(rendered, key="native-schema-scope",
                             text="Read-only comparison of table columns. No SQL is applied.")
        self.expect_rendered(rendered, key="native-schema-missing-0",
                             text="Missing on target: source_note")
        self.expect_rendered(rendered, key="native-schema-extra-0",
                             text="Extra on target: target_note")
        self.expect_rendered(rendered, key="native-schema-changed-0",
                             text="Changed on target: amount (type: source INTEGER, target TEXT)")
        self.assertFalse(visible(rendered, key="native-transfer-yes"))
        self.assertFalse(visible(rendered, key="native-schema-equal"))
        expect_unchanged()

        equal = self.replay([
            *diff_steps, tap(6.4, "native-transfer-source-table"),
            tap(7.0, "native-transfer-target-table"), tap(8.2, "native-schema-compare"),
        ], until=10.2)
        self.expect_rendered(equal, key="native-schema-equal", text="No column differences.")
        self.assertFalse(visible(equal, key="native-schema-results"))
        self.assertFalse(visible(equal, key="native-transfer-yes"))
        expect_unchanged()
