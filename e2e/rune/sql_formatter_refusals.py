#!/usr/bin/env python3
"""Replay Native Format refusal, Undo, and retry against disposable connections."""

import argparse
import json
import unittest
from pathlib import Path

from offline import key
from scenarios import query_steps, tap, visible
from sql_formatter import SqlFormatterJourneys, draft, node, selection
from tcp_sql import DisposableSql


# A successful Format makes a visible change to the earlier, valid statement.
# The malformed second statement must veto the whole operation, not just its
# own rewrite. One Undo after the refusal removes only the second paste.
PREFIX = "select  2;"
FORMATTED_PREFIX = "SELECT\n  2;"
CASES = {
    "sqlite": {
        "missing_group_closer": "select abs(1;",
        "missing_case_then": "select case when 1 end;",
        "empty_case_condition": "select case when then 1 end;",
        "missing_between_upper": "select n from t where n between 1 and;",
        "unfinished_unicode_literal":
            "select café, 列, α٢, 7.0, 1e2 from 客户;\n"
            + "select '" + "α" * 120 + ("\n" + "a" * 88) * 45,
        "invalid_unicode_operator": "select cliente from 客户 where cliente 🙂 2;",
        "unterminated_comment": "select 1 /* unfinished",
    },
    "postgresql": {
        "unimplemented_explain_option": "explain (generic_plan true) select 1;",
        "duplicate_explain_option": "explain (verbose true, verbose false) select 1;",
        "multi_character_copy_delimiter":
            "copy public.rune_e2e_fixture(marker) to stdout with (format csv, delimiter 'ab');",
        "expression_in_copy_columns":
            "copy public.rune_e2e_fixture(marker+1) to stdout with (format csv);",
        "opaque_explain_options":
            "explain /* sql-formatter-disable */ /* notes */ /* sql-formatter-enable */ select 1;",
        "unfinished_nested_comment":
            "select 1 /* outer /* inner */" + ("\n" + "note " * 17) * 50,
        "broken_explain_separator":
            "explain (verbose /* context */ true costs true) select 1;",
    },
}


def main():
    import run as native

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("engine", choices=tuple(CASES))
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--case", action="append",
                        choices=sorted({name for group in CASES.values() for name in group}))
    parser.add_argument("--output-bound", action="store_true",
                        help="run only the SQLite visual refusal for a compact input with oversized output")
    args = parser.parse_args()
    if (args.engine == "sqlite") != (args.fixture is None):
        parser.error("PostgreSQL requires a marked loopback fixture; SQLite creates its own")
    if args.output_bound and (args.engine != "sqlite" or args.case):
        parser.error("--output-bound requires SQLite and cannot be combined with --case")
    if args.case and not set(args.case) <= CASES[args.engine].keys():
        parser.error("--case must belong to this engine")
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if native.ARTIFACTS:
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)

    class Refusals(SqlFormatterJourneys, native.NativeJourneys):
        def run_refusal(self, invalid, *, notice=None):
            pg = None
            if args.engine == "sqlite":
                baseline = self._formatter_fixture({})
                connection = "e2e-sqlite"
            else:
                pg = DisposableSql("postgresql", args.fixture)
                self.assertIn("PostgreSQL ", pg.query("SELECT VERSION();"))
                # The Native password-store helper is not a rendered journey.
                profile = self.env.pop("LLVM_PROFILE_FILE", None)
                try:
                    if profile is not None:
                        self.env["LLVM_PROFILE_FILE"] = str(self.root / "credential-setup.profraw")
                    pg.write_profile(self, read_only=True)
                finally:
                    if profile is not None:
                        self.env["LLVM_PROFILE_FILE"] = profile
                connection = "e2e-tcp-sql"
                settings = self.root / "settings.json"
                settings.write_text('{"ui":{"language":"en"}}', encoding="utf-8")
                original_settings = settings.read_bytes()

            route = [
                *query_steps("", connection)[:4],
                {"t": 4.8, "type": "paste", "text": PREFIX},
                {"t": 5.35, "type": "paste", "text": "\n" + invalid},
                tap(6.1, "sql-format"),
            ]
            # Additional settling time is for the renderer reflowing the
            # big paste, not for relaxing the worker's bound.
            settle = 1.2 if notice else 0
            if settle:
                route[-1] = tap(6.1 + settle, "sql-format")
            refused = self.replay(route, until=7.2 + settle)
            self.expect_rendered(refused, key="sql-format")
            self.assertTrue(node(refused, "sql-editor-scroll").get("focused"))
            self.assertEqual(draft(refused), PREFIX + "\n" + invalid)
            self.assertEqual(selection(refused), [])
            if notice:
                self.expect_rendered(refused, text=notice, within="query-notice")
            else:
                self.assertFalse(visible(refused, key="query-notice"), "parse refusal is silent")

            # This is one continuous undo-and-retry route, not a formatter
            # bypass/reset. If refusal created a ghost undo or mutated the
            # draft, one Undo cannot make the next Format yield this result.
            recovered = self.replay([
                *route,
                key(7.24 + settle, "z", cmd=True),
                tap(7.93 + settle, "sql-format"),
            ], until=9.2 + settle)
            self.assertEqual(draft(recovered), FORMATTED_PREFIX)
            self.assertEqual(selection(recovered), [])
            self.assertFalse(visible(recovered, key="query-notice"))

            if pg is None:
                self._no_query_or_settings_write((refused, recovered), baseline)
            else:
                for tree in (refused, recovered):
                    self.assertFalse(visible(tree, key="result-workspace"))
                    self.assertFalse(visible(tree, key="query-error-message"))
                history = self.root / "query-history.json"
                self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
                self.assertEqual(settings.read_bytes(), original_settings)
                self.assertEqual(pg.query("SELECT marker FROM rune_e2e_fixture;"),
                                 "datazen-rune-disposable")

        def test_output_bound(self):
            # Long generated tuple in a nested expression. Moving each item
            # to an indented line expands the output beyond 512 KiB, while
            # the exact authored input is small and has a low token count.
            inner = ("1," * 40 + "\n") * 300 + "1"
            large = "select " + "(" * 22 + inner + ")" * 22 + ";"
            self.assertLess(len(large.encode("utf-8")), 32 * 1024)
            self.run_refusal(large, notice="This SQL is too large to format.")

    suite = unittest.TestSuite()
    if args.output_bound:
        suite.addTest(Refusals("test_output_bound"))
    for name, invalid in (() if args.output_bound else CASES[args.engine].items()):
        if args.case is None or name in args.case:
            method = "test_" + name
            setattr(Refusals, method, lambda self, sql=invalid: self.run_refusal(sql))
            suite.addTest(Refusals(method))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
