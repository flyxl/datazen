#!/usr/bin/env python3
"""Exercise rejected drafts and Undo recovery in authenticated MySQL-family tabs.

Run against a marked, disposable host with the server's general query log enabled.
The CLI only reads the marker and version. The Native toolbar never sends the drafts.
"""
import argparse
import json
from pathlib import Path
import unittest

from offline import key
from scenarios import query_steps, tap, visible
from sql_formatter import draft
from tcp_sql import DisposableSql


# Each witness represents a normal intermediate editor draft or a client-script
# feature. A leading format-able SELECT catches accidental partial rewrites.
CASES = {
    "mysql": (
        ("client_directive", "DELIMITER //\ncreate procedure rune_fmt_refusal_p() select 1 //"),
        ("index_missing_columns", "create index rune_fmt_refusal_i on ledger;"),
        ("view_missing_query", "create view rune_fmt_refusal_v as;"),
        ("unsafe_explain_target", "explain update rune_fmt_refusal_ex set n=1;"),
        ("incomplete_duplicate_key", "insert into rune_fmt_refusal_m(id,n) values(1,2) "
                                    "on duplicate key update n;"),
    ),
    "mariadb": (
        ("create_missing_definition", "create table rune_fmt_refusal_t;"),
        ("drop_missing_target", "drop index rune_fmt_refusal_drop on;"),
        ("delete_multiple_targets", "delete a,b from rune_fmt_refusal_delete a join ledger b on a.id=b.id;"),
        ("values_missing_tuple", "insert into rune_fmt_refusal_values(n) values(1),;"),
        ("add_index_missing_columns", "alter table rune_fmt_refusal_a add index name_key;"),
    ),
}

# The repaired statements' exact text comes from the pinned 15.8.2 MySQL and
# MariaDB command oracles, respectively. The refused input is an intentional
# Native no-edit protection, not a claim that the original parser refused it.
REPAIRS = {
    "mysql": (
        "insert into rune_fmt_refusal_m(id,n) values(1,2) on duplicate key update n=3;",
        "INSERT INTO\n  rune_fmt_refusal_m (id, n)\nVALUES\n  (1, 2)\n"
        "ON DUPLICATE KEY UPDATE\n  n = 3;",
    ),
    "mariadb": (
        "alter table rune_fmt_refusal_a add index name_key(n);",
        "ALTER TABLE rune_fmt_refusal_a\nADD INDEX name_key (n);",
    ),
}

PREFIX = "select  1;"


def main():
    import run as native

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("engine", choices=tuple(CASES))
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--server-log", required=True, type=Path)
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if args.artifacts:
        args.artifacts.mkdir(parents=True, exist_ok=True)

    class FormatterRefusal(native.NativeJourneys):
        def setUp(self):
            super().setUp()
            self.db_fixture = DisposableSql(args.engine, args.fixture)
            version = self.db_fixture.query("SELECT VERSION();").lower()
            self.assertEqual("mariadb" in version, args.engine == "mariadb")
            if args.engine == "mysql":
                self.assertIn("mysql", self.db_fixture.query("SELECT @@version_comment;").lower())
            self.log_offset = args.server_log.stat().st_size
            # Native CLI fixture setup is not a rendered journey or input to
            # the formatter's UI-only profile collection.
            profile_file = self.env.get("LLVM_PROFILE_FILE")
            if profile_file is not None:
                self.env["LLVM_PROFILE_FILE"] = str(self.root / "credential-setup.profraw")
            try:
                self.db_fixture.write_profile(self, read_only=True)
            finally:
                if profile_file is not None:
                    self.env["LLVM_PROFILE_FILE"] = profile_file
            self.settings = self.root / "settings.json"
            self.settings.write_text(json.dumps({
                "ui": {"language": "en"},
                "sqlFormatOptions": {"keywordCase": "upper", "indentStyle": "2spaces"},
            }), encoding="utf-8")
            self.original_settings = self.settings.read_bytes()

        def _route(self, sample):
            original = PREFIX + "\n" + sample
            return original, [
                *query_steps("", "e2e-tcp-sql")[:4],
                {"t": 4.9, "type": "paste", "text": original},
                tap(6.25, "sql-format"),
            ]

        def _expect_no_sql(self, tree):
            # Parse and unsupported refusals are deliberately silent. A
            # timeout, disabled provider, query error or result is not a pass.
            for forbidden in ("query-notice", "query-error-message", "result-workspace"):
                self.assertFalse(visible(tree, key=forbidden), forbidden)
            history = self.root / "query-history.json"
            self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
            self.assertEqual(self.settings.read_bytes(), self.original_settings)
            self.assertEqual(self.db_fixture.query("SELECT marker FROM rune_e2e_fixture;"),
                             "datazen-rune-disposable")
            with args.server_log.open("rb") as log:
                log.seek(self.log_offset)
                fresh = log.read().lower()
            self.assertIn(b"information_schema", fresh, "missing live Native metadata traffic")
            self.assertNotIn(b"rune_fmt_refusal_", fresh, "draft SQL reached the server")

        def check_refusal(self, sample):
            original, route = self._route(sample)
            refused = self.replay(route, until=8.35)
            self.assertEqual(draft(refused), original, "rejected Format changed editor content")
            self._expect_no_sql(refused)

        def test_repair_after_refusal_preserves_the_real_undo(self):
            _, route = self._route(CASES[args.engine][-1][1])
            repair, expected = REPAIRS[args.engine]
            repaired = self.replay([
                *route, key(8.75, "z", cmd=True),
                # No Select All before this paste: Undo must remove the first
                # paste, rather than undoing a phantom Format edit.
                {"t": 9.3, "type": "paste", "text": repair},
                tap(10.15, "sql-format"),
            ], until=12.2)
            self.assertEqual(draft(repaired), expected)
            self._expect_no_sql(repaired)

    suite = unittest.TestSuite()
    for case_id, sample in CASES[args.engine]:
        name = "test_refuses_" + case_id
        setattr(FormatterRefusal, name, lambda self, sql=sample: self.check_refusal(sql))
        suite.addTest(FormatterRefusal(name))
    suite.addTest(FormatterRefusal("test_repair_after_refusal_preserves_the_real_undo"))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
