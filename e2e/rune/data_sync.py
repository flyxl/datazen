"""Read-only native Data Sync previews with independent SQLite oracles."""
from contextlib import closing
import json
import sqlite3

from scenarios import tap, visible


class DataSyncJourneys:
    def sync_profiles(self, target):
        self.store.write_text(json.dumps({"connections": [
            {"id": "sync-src", "name": "Sync source", "databaseType": "sqlite",
             "database": str(self.db), "connectionTimeout": 8},
            {"id": "sync-dst", "name": "Sync target", "databaseType": "sqlite",
             "database": str(target), "connectionTimeout": 8},
        ]}), encoding="utf-8")

    def sync_steps(self):
        return [
            tap(.5, "rail-tools"), tap(1.2, "native-tool-data-sync"),
            tap(2.0, "native-transfer-source-profile-sync-src"),
            tap(2.6, "native-transfer-target-profile-sync-dst"),
        ]

    def expect_sync_files_unchanged(self, originals):
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)
            for suffix in ("-wal", "-shm", "-journal"):
                self.assertFalse(path.with_name(path.name + suffix).exists())
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                self.assertEqual(db.execute("PRAGMA journal_mode").fetchone(), ("delete",))

    def test_sqlite_sync_preview_counts_real_row_differences_without_writes(self):
        target = self.root / "sync-target.sqlite"
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript("""
                CREATE TABLE a_no_pk (payload TEXT);
                CREATE TABLE "z.items" (id INTEGER PRIMARY KEY, payload TEXT, weight INTEGER);
                INSERT INTO "z.items" VALUES
                  (1, NULL, 11), (2, NULL, 22), (4, 'new', 44), (7, 'another new', 77),
                  (12, 'equal', NULL), (15, '', 15);
            """)
        with closing(sqlite3.connect(target)) as db:
            db.executescript("""
                CREATE TABLE a_no_pk (payload TEXT);
                CREATE TABLE "z.items" (weight INTEGER, payload TEXT, id INTEGER PRIMARY KEY);
                INSERT INTO "z.items" (id, payload, weight) VALUES
                  (1, NULL, 11), (2, '', 22), (3, 'keep 3', 33), (5, 'keep 5', 55),
                  (8, 'keep 8', 88), (9, 'keep 9', 99), (12, 'equal', NULL), (15, '', 15);
            """)
        self.sync_profiles(target)
        originals = {path: path.read_bytes() for path in (self.db, target)}

        def oracle():
            with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
                db.execute("ATTACH DATABASE ? AS dest", (f"{target.as_uri()}?mode=ro",))
                return db.execute("""
                    SELECT
                      (SELECT count(*) FROM main."z.items" s
                       WHERE NOT EXISTS (SELECT 1 FROM dest."z.items" t WHERE t.id=s.id)),
                      (SELECT count(*) FROM main."z.items" s JOIN dest."z.items" t USING (id)
                       WHERE s.weight IS NOT t.weight OR s.payload IS NOT t.payload),
                      (SELECT count(*) FROM main."z.items" s JOIN dest."z.items" t USING (id)
                       WHERE s.weight IS t.weight AND s.payload IS t.payload),
                      (SELECT count(*) FROM dest."z.items" t
                       WHERE NOT EXISTS (SELECT 1 FROM main."z.items" s WHERE s.id=t.id))
                """).fetchone()

        counts = oracle()
        self.assertEqual(counts, (2, 1, 3, 4))
        rejected = self.replay([*self.sync_steps(), tap(4.0, "native-sync-compare")], until=5.6)
        self.expect_rendered(rejected, key="native-sync-error")
        self.assertTrue(any(
            node.get("key") == "native-sync-error"
            and "both tables must have a PRIMARY KEY" in (node.get("text") or "")
            for node in rejected["elements"]
        ))
        self.assertFalse(visible(rejected, key="native-sync-results"))
        self.expect_sync_files_unchanged(originals)

        tree = self.replay([
            *self.sync_steps(), tap(3.8, "native-transfer-source-table"),
            tap(4.4, "native-transfer-target-table"), tap(5.7, "native-sync-compare"),
        ], until=7.5)
        self.expect_rendered(tree, key="native-sync-scope")
        for suffix, caption, count in zip(
            ("insert", "update", "unchanged", "target-only"),
            ("Would insert", "Would update", "Unchanged", "Target-only (kept)"), counts,
        ):
            self.expect_rendered(tree, key=f"native-sync-{suffix}", text=f"{caption}: {count}")
        self.assertFalse(visible(tree, key="native-sync-error"))
        self.assertFalse(visible(tree, key="native-transfer-yes"))
        self.assertEqual(oracle(), counts)
        self.expect_sync_files_unchanged(originals)

    def test_sqlite_sync_preview_refuses_to_report_partial_counts_beyond_limit(self):
        target = self.root / "sync-limit.sqlite"
        expected = {self.db: 1, target: 1001}
        for path, count in expected.items():
            with closing(sqlite3.connect(path)) as db:
                db.execute("CREATE TABLE records (id INTEGER PRIMARY KEY)")
                db.executemany("INSERT INTO records (id) VALUES (?)",
                               ((value,) for value in range(1, count + 1)))
                db.commit()
        self.sync_profiles(target)
        originals = {path: path.read_bytes() for path in expected}
        tree = self.replay([*self.sync_steps(), tap(4.0, "native-sync-compare")], until=6)
        self.expect_rendered(tree, key="native-sync-error",
                             text="Data Sync preview supports at most 1000 rows per table.")
        self.assertFalse(visible(tree, key="native-sync-results"))
        self.expect_sync_files_unchanged(originals)
        for path, count in expected.items():
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM records").fetchone(), (count,))

    def test_sqlite_sync_preview_compares_composite_keys_at_exact_row_limit(self):
        target = self.root / "sync-boundary.sqlite"
        schema = """
            CREATE TABLE records (
                region TEXT NOT NULL,
                id INTEGER NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (region, id)
            ) WITHOUT ROWID
        """
        with closing(sqlite3.connect(self.db)) as db:
            db.execute(schema)
            db.executemany("INSERT INTO records VALUES (?, ?, ?)", [
                *(("A", value, f"row {value}") for value in range(1, 1000)),
                ("a", 1, "source lowercase"),
            ])
            db.commit()
        with closing(sqlite3.connect(target)) as db:
            db.execute(schema)
            db.executemany("INSERT INTO records VALUES (?, ?, ?)", [
                ("A", 1, "row 1"), ("a", 1, "target lowercase"), ("Z", 1, "target only")
            ])
            db.commit()
        self.sync_profiles(target)
        originals = {path: path.read_bytes() for path in (self.db, target)}

        def records(path):
            with closing(sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)) as db:
                return {(region, row_id): value for region, row_id, value in
                        db.execute("SELECT region, id, value FROM records")}

        source_rows, target_rows = records(self.db), records(target)
        self.assertEqual(len(source_rows), 1000)
        common = source_rows.keys() & target_rows.keys()
        expected = {
            "insert": ("Would insert", len(source_rows.keys() - target_rows.keys())),
            "update": ("Would update", sum(source_rows[key] != target_rows[key] for key in common)),
            "unchanged": ("Unchanged", sum(source_rows[key] == target_rows[key] for key in common)),
            "target-only": ("Target-only (kept)", len(target_rows.keys() - source_rows.keys())),
        }
        self.assertEqual([count for _, count in expected.values()], [998, 1, 1, 1])
        tree = self.replay([*self.sync_steps(), tap(4.0, "native-sync-compare")], until=6)
        for suffix, (caption, count) in expected.items():
            self.expect_rendered(tree, key=f"native-sync-{suffix}", text=f"{caption}: {count}")
        self.assertFalse(visible(tree, key="native-sync-error"))
        self.expect_sync_files_unchanged(originals)
        self.assertEqual((records(self.db), records(target)), (source_rows, target_rows))

    def test_sqlite_sync_preview_clears_stale_counts_and_refuses_file_alias(self):
        target = self.root / "sync-distinct.sqlite"
        alias = self.root / "source-hardlink.sqlite"
        for path, row in ((self.db, (1, "from source")), (target, (2, "keep target"))):
            with closing(sqlite3.connect(path)) as db:
                db.execute("CREATE TABLE records (id INTEGER PRIMARY KEY, note TEXT)")
                db.execute("INSERT INTO records VALUES (?, ?)", row)
                db.commit()
        alias.hardlink_to(self.db)
        self.sync_profiles(target)
        saved = json.loads(self.store.read_text(encoding="utf-8"))
        saved["connections"].append({
            "id": "sync-alias", "name": "Source alias", "databaseType": "sqlite",
            "database": str(alias), "connectionTimeout": 8,
        })
        self.store.write_text(json.dumps(saved), encoding="utf-8")
        originals = {path: path.read_bytes() for path in (self.db, target, alias)}
        setup = [*self.sync_steps(), tap(4.0, "native-sync-compare")]

        initial = self.replay(setup, until=5.6)
        self.expect_rendered(initial, key="native-sync-insert", text="Would insert: 1")
        self.expect_rendered(initial, key="native-sync-target-only", text="Target-only (kept): 1")

        reopened = self.replay([
            *setup, tap(6.0, "native-tool-transfer"), tap(6.8, "native-tool-data-sync"),
        ], until=8.0)
        self.expect_rendered(reopened, key="native-sync-compare")
        self.assertFalse(visible(reopened, key="native-sync-results"))

        rejected = self.replay([
            *setup, tap(6.0, "native-transfer-target-profile-sync-alias"),
            tap(7.65, "native-sync-compare"),
        ], until=9.0)
        self.expect_rendered(rejected, key="native-sync-error",
                             text="Select two different SQLite files.")
        self.assertFalse(visible(rejected, key="native-sync-results"))
        self.expect_sync_files_unchanged(originals)
