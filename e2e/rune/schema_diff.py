"""Read-only native schema comparison against two actual SQLite catalogs."""
from contextlib import closing
import json
import sqlite3

from scenarios import tap, visible, open_tool_steps


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
        setup = [*open_tool_steps(.5, 1.2, "schema-diff"),
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
                             text="Read-only comparison of columns, named CHECK constraints and ordinary secondary indexes. No SQL is applied.")
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
        self.expect_rendered(equal, key="native-schema-equal", text="No differences in the compared columns, named CHECK constraints or ordinary secondary indexes.")
        self.assertFalse(visible(equal, key="native-schema-results"))
        self.assertFalse(visible(equal, key="native-transfer-yes"))
        expect_unchanged()

    def test_sqlite_schema_diff_detects_nullability_and_default_without_ddl(self):
        target = self.root / "column-options.sqlite"
        for path, table in (
            (self.db, "CREATE TABLE records (id INTEGER PRIMARY KEY, label TEXT NOT NULL DEFAULT 'source')"),
            (target, "CREATE TABLE records (id INTEGER PRIMARY KEY, label TEXT DEFAULT 'target')"),
        ):
            with closing(sqlite3.connect(path)) as connection:
                connection.execute(table)
                connection.execute("INSERT INTO records (id) VALUES (1)")
                connection.commit()
        before = {path: path.read_bytes() for path in (self.db, target)}

        def profile(identifier, name, path):
            return {"id": identifier, "name": name, "databaseType": "sqlite",
                    "database": str(path), "connectionTimeout": 8}

        self.store.write_text(json.dumps({"connections": [
            profile("options-src", "Options Source", self.db),
            profile("options-dst", "Options Target", target),
        ]}), encoding="utf-8")
        setup = [*open_tool_steps(.5, 1.2, "schema-diff")]
        empty = self.replay([*setup, tap(2.0, "native-schema-compare")], until=3)
        self.expect_rendered(empty, key="native-schema-error",
                             text="Select an existing source and target table.")
        self.assertFalse(visible(empty, key="native-schema-results"))
        rendered = self.replay([
            *setup, tap(2.0, "native-transfer-source-profile-options-src"),
            tap(2.6, "native-transfer-target-profile-options-dst"),
            tap(4.1, "native-schema-compare"),
        ], until=6.2)
        self.expect_rendered(rendered, key="native-schema-changed-0",
                             text="Changed on target: label (nullability, default)")
        for key in ("native-schema-missing-0", "native-schema-extra-0", "native-schema-changed-1",
                    "native-schema-error", "native-transfer-yes"):
            self.assertFalse(visible(rendered, key=key), f"Unexpected rendered node {key}")
        for path, required, default in ((self.db, 1, "source"), (target, 0, "target")):
            self.assertEqual(path.read_bytes(), before[path])
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                self.assertEqual(db.execute("""
                    SELECT [notnull], dflt_value FROM pragma_table_info('records') WHERE name = 'label'
                """).fetchall(), [(required, f"'{default}'")])
                self.assertEqual(db.execute("SELECT label FROM records").fetchall(), [(default,)])

    def test_sqlite_schema_diff_compares_real_named_checks_and_secondary_indexes(self):
        target = self.root / "schema-objects.sqlite"
        self.seed_schema_comparison(target, """
            CREATE TABLE records (
                id INTEGER PRIMARY KEY, amount INTEGER, label TEXT UNIQUE,
                CONSTRAINT amount_floor CHECK (amount > 0),
                CONSTRAINT source_label CHECK (label <> '')
            );
            CREATE UNIQUE INDEX idx_amount_label ON records (amount, label);
            CREATE INDEX idx_source ON records (label);
            INSERT INTO records VALUES (1, 5, 'source');
        """, """
            CREATE TABLE records (
                id INTEGER PRIMARY KEY, amount INTEGER, label TEXT UNIQUE,
                CONSTRAINT amount_floor CHECK (amount >= 0),
                CONSTRAINT target_label CHECK (length(label) > 1)
            );
            CREATE INDEX idx_amount_label ON records (label, amount);
            CREATE INDEX idx_target ON records (label);
            INSERT INTO records VALUES (2, 0, 'target');
        """)

        # SQLite's raw DDL and native index metadata, not driver DTOs.
        before = self.snapshot_schema_comparison(target)
        (_, (source_ddl, source_indexes, source_rows, _)), (
            _, (target_ddl, target_indexes, target_rows, _)
        ) = before
        self.assertIn('CONSTRAINT amount_floor CHECK (amount > 0)', dict(source_ddl)['records'])
        self.assertIn("CONSTRAINT source_label CHECK (label <> '')", dict(source_ddl)['records'])
        self.assertIn('CONSTRAINT amount_floor CHECK (amount >= 0)', dict(target_ddl)['records'])
        self.assertIn('CONSTRAINT target_label CHECK (length(label) > 1)', dict(target_ddl)['records'])
        self.assertEqual(source_indexes, [
            ("idx_amount_label", 1, "c", 0, [("amount", 0, "BINARY"), ("label", 0, "BINARY")]),
            ("idx_source", 0, "c", 0, [("label", 0, "BINARY")]),
            ("sqlite_autoindex_records_1", 1, "u", 0, [("label", 0, "BINARY")]),
        ])
        self.assertEqual(target_indexes, [
            ("idx_amount_label", 0, "c", 0, [("label", 0, "BINARY"), ("amount", 0, "BINARY")]),
            ("idx_target", 0, "c", 0, [("label", 0, "BINARY")]),
            ("sqlite_autoindex_records_1", 1, "u", 0, [("label", 0, "BINARY")]),
        ])
        self.assertEqual((source_rows, target_rows), ([(1, 5, "source")], [(2, 0, "target")]))
        rendered = self.replay(self.schema_comparison_steps(), until=6.2)
        for key, expected in (
            ("check-expected-0", "Expected on target: CHECK amount_floor (amount > 0)"),
            ("check-expected-1", "Expected on target: CHECK source_label (label <> '')"),
            ("check-unexpected-0", "Unexpected on target: CHECK amount_floor (amount >= 0)"),
            ("check-unexpected-1", "Unexpected on target: CHECK target_label (length(label) > 1)"),
            ("index-expected-0", "Expected on target: Index idx_amount_label (unique (amount, label))"),
            ("index-expected-1", "Expected on target: Index idx_source (non-unique (label))"),
            ("index-unexpected-0", "Unexpected on target: Index idx_amount_label (non-unique (label, amount))"),
            ("index-unexpected-1", "Unexpected on target: Index idx_target (non-unique (label))"),
        ):
            self.expect_rendered(rendered, key=f"native-schema-{key}", text=expected)
        for key in ("changed-0", "equal", "error", "unresolved", "index-expected-2",
                    "index-unexpected-2"):
            self.assertFalse(visible(rendered, key=f"native-schema-{key}"), key)
        self.assertFalse(visible(rendered, key="native-transfer-yes"))
        self.assertEqual(self.snapshot_schema_comparison(target), before)

    def test_sqlite_schema_diff_warns_when_partial_indexes_and_ambiguous_checks_are_unresolved(self):
        target = self.root / "partial-schema.sqlite"
        self.seed_schema_comparison(target, """
            CREATE TABLE records (id INTEGER PRIMARY KEY, amount INTEGER, label TEXT,
                                  CONSTRAINT source_floor CHECK (amount > 0));
            CREATE INDEX idx_scope ON records (amount) WHERE amount > 0;
            INSERT INTO records VALUES (1, 5, 'source');
        """, """
            CREATE TABLE records (id INTEGER PRIMARY KEY, amount INTEGER, label TEXT,
                                  CHECK (amount >= 0),
                                  CONSTRAINT repeated CHECK (label <> ''),
                                  CONSTRAINT repeated CHECK (length(label) < 10));
            CREATE INDEX idx_scope ON records (amount) WHERE amount >= 0;
            INSERT INTO records VALUES (2, 0, 'target');
        """)
        before = self.snapshot_schema_comparison(target)
        (_, (source_ddl, source_indexes, _, _)), (_, (target_ddl, target_indexes, _, _)) = before
        self.assertIn("WHERE amount > 0", dict(source_ddl)["idx_scope"])
        self.assertIn("WHERE amount >= 0", dict(target_ddl)["idx_scope"])
        self.assertEqual(dict(target_ddl)["records"].count("CONSTRAINT repeated CHECK"), 2)
        self.assertIn("CHECK (amount >= 0)", dict(target_ddl)["records"])
        # Same projected index keys, genuinely different excluded predicates.
        self.assertEqual(source_indexes, target_indexes)
        self.assertEqual(target_indexes, [
            ("idx_scope", 0, "c", 1, [("amount", 0, "BINARY")]),
        ])
        rendered = self.replay(self.schema_comparison_steps(), until=6.2)
        self.expect_rendered(rendered, key="native-schema-unresolved",
                             text="Comparison incomplete: CHECK constraints and indexes were not compared.")
        for index, message in enumerate((
            "Source: Partial index `idx_scope` is not represented by Schema Diff",
            "Target: Partial index `idx_scope` is not represented by Schema Diff",
            "Target: Repeated CHECK name `repeated` cannot be compared.",
            "Target: Unnamed or ambiguously named CHECK constraints cannot be compared.",
        )):
            self.expect_rendered(rendered, key=f"native-schema-unresolved-{index}", text=message)
        for key in ("changed-0", "equal", "error", "index-expected-0", "index-unexpected-0",
                    "check-expected-0", "check-unexpected-0"):
            self.assertFalse(visible(rendered, key=f"native-schema-{key}"), key)
        self.assertFalse(visible(rendered, key="native-transfer-yes"))
        self.assertEqual(self.snapshot_schema_comparison(target), before)

    def seed_schema_comparison(self, target, source_sql, target_sql):
        for path, sql in ((self.db, source_sql), (target, target_sql)):
            with closing(sqlite3.connect(path)) as db:
                db.executescript(sql)
        self.store.write_text(json.dumps({"connections": [{
            "id": name, "name": name, "databaseType": "sqlite",
            "database": str(path), "readOnly": read_only, "connectionTimeout": 8,
        } for name, path, read_only in (
            ("objects-src", self.db, False), ("objects-dst", target, True),
        )]}), encoding="utf-8")

    def snapshot_schema_comparison(self, target):
        def snapshot(path):
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                ddl = db.execute("""
                    SELECT name, sql FROM sqlite_schema
                    WHERE tbl_name = 'records' ORDER BY name
                """).fetchall()
                indexes = [(*index, db.execute("""
                    SELECT name, [desc], coll FROM pragma_index_xinfo(?)
                    WHERE key = 1 ORDER BY seqno
                """, (index[0],)).fetchall()) for index in db.execute("""
                    SELECT name, [unique], origin, partial
                    FROM pragma_index_list('records') ORDER BY name
                """)]
                rows = db.execute("SELECT * FROM records ORDER BY id").fetchall()
                journal_mode = db.execute("PRAGMA journal_mode").fetchone()
            for suffix in ("-wal", "-shm", "-journal"):
                self.assertFalse(path.with_name(path.name + suffix).exists())
            return path.read_bytes(), (ddl, indexes, rows, journal_mode)

        return [snapshot(path) for path in (self.db, target)]

    def schema_comparison_steps(self):
        return [*open_tool_steps(.5, 1.2, "schema-diff"),
                tap(2.0, "native-transfer-source-profile-objects-src"),
                tap(2.6, "native-transfer-target-profile-objects-dst"),
                tap(4.1, "native-schema-compare")]
