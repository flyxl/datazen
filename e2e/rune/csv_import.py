"""Real native CSV review and transactional SQLite append without external services."""
from contextlib import closing
import json
import sqlite3

from offline import DATA_SETTINGS, SETTINGS, fill
from scenarios import tap, visible


class CsvImportJourneys:
    def csv_import_fixture(self):
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript("""
                CREATE TABLE "q.records"(
                    id INTEGER PRIMARY KEY,
                    note TEXT NOT NULL,
                    tag TEXT,
                    memo TEXT DEFAULT 'original default'
                );
                INSERT INTO "q.records" VALUES (9, 'already here', 'retained', 'old memo');
            """)
        self.store.write_text(json.dumps({"connections": [
            {"id": "csv-dst", "name": "Writable fixture", "databaseType": "sqlite",
             "database": str(self.db), "connectionTimeout": 8},
            {"id": "csv-ro", "name": "Read-only fixture", "databaseType": "sqlite",
             "database": str(self.db), "readOnly": True, "connectionTimeout": 8},
        ]}), encoding="utf-8")

    def csv_steps(self, source, profile="csv-dst"):
        return [
            SETTINGS, DATA_SETTINGS, *fill(2.35, "import-source-input", str(source)),
            tap(5.6, "import-next"), tap(6.35, f"native-csv-profile-{profile}"),
            *fill(7.0, "import-target-input", "q.records"), tap(8.35, "import-finish"),
        ]

    def csv_target_rows(self):
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            return db.execute("""
                SELECT id, note, tag, typeof(tag), memo FROM "q.records" ORDER BY id
            """).fetchall()

    def expect_csv_unchanged(self, before, originals):
        self.assertEqual(self.csv_target_rows(), before)
        self.assertEqual(self.db.read_bytes(), originals)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

    def test_native_csv_import_reviews_cancels_and_appends_real_sqlite_rows(self):
        self.csv_import_fixture()
        with closing(sqlite3.connect(self.db)) as db:
            db.execute('ALTER TABLE "q.records" ADD COLUMN "Ä" TEXT')
            db.execute('ALTER TABLE "q.records" ADD COLUMN "ä" TEXT')
        source = self.root / "new.csv"
        incoming = 'id,note,tag,Ä,ä\r\n1,"O\'Brien said ""你好"", and left","",first,second\r\n2,"café, 東京",,third,fourth\r\n'
        source.write_bytes(incoming.encode("utf-8"))
        before = [(9, "already here", "retained", "text", "old memo")]
        original = self.db.read_bytes()
        self.expect_csv_unchanged(before, original)

        initial = self.csv_steps(source)
        pending = self.replay(initial, until=10)
        self.expect_rendered(pending, key="native-csv-review", text="new.csv → Writable fixture / q.records")
        self.expect_rendered(pending, key="native-csv-count", text="Rows to append: 2")
        for index, heading in enumerate(("id", "note", "tag", "Ä", "ä")):
            self.expect_rendered(pending, key=f"native-csv-column-{index}", text=f"{heading} → {heading}")
        self.expect_csv_unchanged(before, original)

        cancelled = self.replay([*initial, tap(10.4, "native-csv-cancel")], until=11.6)
        self.expect_rendered(cancelled, key="import-finish")
        self.assertFalse(visible(cancelled, key="native-csv-success"))
        self.assertFalse(visible(cancelled, key="native-csv-review"))
        self.expect_csv_unchanged(before, original)

        readonly = self.replay(self.csv_steps(source, "csv-ro"), until=10.1)
        self.expect_rendered(readonly, key="import-error", text="The CSV import target is read-only.")
        self.assertFalse(visible(readonly, key="native-csv-append"))
        self.expect_csv_unchanged(before, original)

        imported = self.replay([*initial, tap(10.4, "native-csv-append")], until=12.0)
        self.expect_rendered(imported, key="native-csv-success", text="Imported 2 rows.")
        self.assertEqual(self.csv_target_rows(), [
            (1, 'O\'Brien said "你好", and left', "", "text", "original default"),
            (2, "café, 東京", None, "null", "original default"),
            *before,
        ])
        with closing(sqlite3.connect(self.db)) as db:
            self.assertEqual(db.execute(
                'SELECT "Ä", "ä" FROM "q.records" WHERE id < 9 ORDER BY id'
            ).fetchall(), [("first", "second"), ("third", "fourth")])
        self.assertEqual(source.read_bytes(), incoming.encode("utf-8"))

    def test_native_csv_import_refuses_malformed_and_rolls_back_later_conflict(self):
        self.csv_import_fixture()
        source = self.root / "bad.csv"
        before = self.csv_target_rows()
        original = self.db.read_bytes()

        source.write_text('id,note\n1,"unterminated\n', encoding="utf-8")
        malformed = self.replay(self.csv_steps(source), until=10)
        self.expect_rendered(malformed, key="import-error",
                             text="The CSV contains an unterminated quoted field.")
        self.assertFalse(visible(malformed, key="native-csv-append"))
        self.expect_csv_unchanged(before, original)

        source.write_text("id,note\n12,would insert first\n9,duplicate key\n", encoding="utf-8")
        conflicted = self.replay([
            *self.csv_steps(source), tap(10.4, "native-csv-append"),
        ], until=12)
        self.expect_rendered(conflicted, key="import-error",
                             text="A CSV row violates the target table constraints.")
        self.assertFalse(visible(conflicted, key="native-csv-success"))
        self.expect_csv_unchanged(before, original)

    def test_native_csv_import_rejects_duplicate_headers_and_target_triggers(self):
        self.csv_import_fixture()
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript("""
                CREATE TABLE import_audit (note TEXT);
                CREATE TRIGGER records_audit AFTER INSERT ON "Q.RECORDS"
                BEGIN INSERT INTO import_audit VALUES (NEW.note); END;
            """)
        source = self.root / "guard.csv"
        before = self.csv_target_rows()
        original = self.db.read_bytes()

        source.write_text("id,note,note\n1,first field,second field\n", encoding="utf-8")
        duplicate = self.replay(self.csv_steps(source), until=10)
        self.expect_rendered(duplicate, key="import-error", text="The CSV has duplicate headers.")
        self.assertFalse(visible(duplicate, key="native-csv-review"))
        self.expect_csv_unchanged(before, original)

        source.write_text("id,note\n2,must not be imported\n", encoding="utf-8")
        guarded = self.replay(self.csv_steps(source), until=10)
        self.expect_rendered(guarded, key="import-error",
                             text="Choose an existing ordinary SQLite table with no triggers.")
        self.assertFalse(visible(guarded, key="native-csv-append"))
        self.expect_csv_unchanged(before, original)
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            self.assertEqual(db.execute("SELECT note FROM import_audit").fetchall(), [])
