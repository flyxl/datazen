#!/usr/bin/env python3
"""Strict Native Format/Undo gate for SQLite or one marked, real TCP engine.

Example: python e2e/rune/sql_formatter_dialects.py BIN postgresql --fixture CONFIG.
The expected drafts are static outputs checked against the original 15.8.2
formatter. No reference engine or test entry point runs in the application.
"""
import argparse
import json
from pathlib import Path
import unittest

from offline import key
from scenarios import query_steps, tap, visible
from sql_formatter import SqlFormatterJourneys, draft
from tcp_sql import DisposableSql


def main(argv=None):
    import run as native

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("engine", choices=("sqlite", "postgresql", "mysql", "mariadb"))
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--case", help="run one named fixture for the selected engine")
    args = parser.parse_args(argv)

    if (args.engine == "sqlite") != (args.fixture is None):
        parser.error("TCP requires a disposable fixture; SQLite must not take one")
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if args.artifacts:
        args.artifacts.mkdir(parents=True, exist_ok=True)

    suite = unittest.TestSuite()
    cases = json.loads(Path(__file__).with_name("sql-formatter-dialects-15.8.2.json")
                       .read_text(encoding="utf-8"))
    if cases["formatter"] != "15.8.2":
        raise ValueError("wrong reference formatter")

    class FormatterDialect(native.NativeJourneys):
        _formatter_fixture = SqlFormatterJourneys._formatter_fixture
        _no_query_or_settings_write = SqlFormatterJourneys._no_query_or_settings_write

        def run_case(self, case):
            prefs = {"sqlFormatOptions": case["options"]}
            if args.engine == "sqlite":
                baseline = self._formatter_fixture(prefs)
                connection = "e2e-sqlite"
            else:
                db = DisposableSql(args.engine, args.fixture)
                # Do not count a MySQL protocol adapter talking to MariaDB
                # (or the reverse) as an end-to-end run of that engine.
                version = db.query("SELECT version();").lower()
                if args.engine == "postgresql":
                    self.assertIn("postgresql", version)
                else:
                    self.assertEqual("mariadb" in version, args.engine == "mariadb")
                    if args.engine == "mysql":
                        self.assertIn("mysql", db.query("SELECT @@version_comment;").lower())
                # Password setup is a separate native CLI entry point, not a
                # rendered journey. Never include its counters in UI coverage.
                original = self.env.get("LLVM_PROFILE_FILE")
                if original is not None:
                    self.env["LLVM_PROFILE_FILE"] = str(self.root / "credential-setup.profraw")
                try:
                    db.write_profile(self, read_only=True)
                finally:
                    if original is not None:
                        self.env["LLVM_PROFILE_FILE"] = original
                connection = "e2e-tcp-sql"
                settings = self.root / "settings.json"
                settings.write_text(json.dumps({"ui": {"language": "en"}, **prefs}), encoding="utf-8")
                original_settings = settings.read_bytes()

            route = [*query_steps("", connection)[:4],
                     {"t": 4.9, "type": "paste", "text": case["input"]}, tap(6.25, "sql-format")]
            formatted = self.replay(route, until=8.55)
            self.assertEqual(draft(formatted), case["expected"])
            self.assertFalse(visible(formatted, key="query-notice"))
            undone = self.replay([*route, key(8.75, "z", cmd=True)], until=9.65)
            self.assertEqual(draft(undone), case["input"], "one Undo restores the entire input")
            if args.engine == "sqlite":
                self._no_query_or_settings_write((formatted, undone), baseline)
            else:
                for tree in (formatted, undone):
                    self.assertFalse(visible(tree, key="result-workspace"))
                    self.assertFalse(visible(tree, key="query-error-message"))
                history = self.root / "query-history.json"
                self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
                self.assertEqual(settings.read_bytes(), original_settings)
                self.assertEqual(db.query("SELECT marker FROM rune_e2e_fixture;"), "datazen-rune-disposable")

    for case in cases["journeys"]:
        if case["engine"] == args.engine and (args.case is None or args.case == case["id"]):
            name = "test_" + case["id"]
            setattr(FormatterDialect, name, lambda self, row=case: self.run_case(row))
            suite.addTest(FormatterDialect(name))
    if not suite.countTestCases():
        raise ValueError("no cases for selected engine")
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
