"""Stock public API external process conformance; not a Native renderer run."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT.parent / "tests" / "reference"
ORACLE = ROOT / "reference_oracle.cjs"
DEFAULTS = dict(language="sql", keywordCase="upper", tabWidth=2, useTabs=False,
                logicalOperatorNewline="before", linesBetweenQueries=1)
ERRORS = dict(Unsupported="unsupported SQL format syntax or dialect",
              Lexical="invalid SQL token", Syntax="invalid SQL syntax",
              InvalidOptions="invalid SQL format options", InputLimit="SQL format input limit exceeded",
              OutputLimit="SQL format output limit exceeded", Deadline="SQL formatting timed out")


def collect_cases():
    result = []
    def case(id, sql, *, language="sql", kind="reference", reason=None, checked=None, **options):
        assert re.fullmatch(r"[a-zA-Z0-9_-]+", id)
        result.append(dict(id=id, sql=sql, jsOptions={**DEFAULTS, "language": language, **options},
                           kind=kind, reason=reason, checked=checked))
    for file in ("golden.json", "golden-ddl.json"):
        data = json.loads((FIXTURES/file).read_text())
        assert data["formatter"] == "15.8.2"
        for row in data["rows"]:
            options = row["options"]
            if options["language"] == "transactsql":
                continue
            if row["expect"]["kind"] == "Formatted":
                values = {k: options[k] for k in DEFAULTS}
                if row["id"] == "mysql-delimiter-syntax":
                    case(row["id"], row["input"], checked=row["expect"]["output"], kind="Unsupported",
                         reason="documented native refusal to rewrite client-side DELIMITER", **values)
                else:
                    case(row["id"], row["input"], checked=row["expect"]["output"], **values)
            elif row["id"] == "generic-invalid-token":
                case(row["id"], row["input"], kind="Lexical",
                     reason="expected lexical rejection in both implementations")
    case("sql-national-and-escaped-literal", "select N'中文', U&'\\0061', X'AB45', 'A\\'B' from \u2009\"Accounts\";")
    case("sql-operators-and-where",
         "select a||b, (a-b)/2, n<>3 from ledger where m!=4 and (id<=5 or id>=-2);",
         tabWidth=4, logicalOperatorNewline="after", linesBetweenQueries=2)
    case("sql-question-marks", "select ?, a, (c + 1) from t where b = ?;")
    case("sql-long-national-literal", "select N'" + "中文" * 2049 + "' as message;")
    case("mysql-import-exponent-punctuation", "select 1e2.3, 7.1e+4.6 as imported;",
         language="mysql", reason="preserve pasted decimal-exponent punctuation exactly like the reference")
    case("sqlite-incomplete-numeric-qualification", "select 1.value_col from import_buffer;",
         language="sqlite", reason="preserve partially edited dot-qualified spelling")
    case("postgresql-comment-before-function-arguments",
         "select coalesce /* exported expression */ (label,'unknown') from contacts;",
         language="postgresql", reason="preserve the authored comment when reflowing a function call")
    case("sqlite-nested-zero-argument-calls-and-between",
         "select abs(random()),coalesce(nullif(1,0),random()) where (2 between 1 and 3);",
         language="sqlite", reason="retain nested calls and grouping with inline spacing")
    case("sql-clause-overview",
         "SELECT dept, sum(score) AS total FROM entries GROUP BY dept HAVING sum(score)>1 "
         "ORDER BY total DESC OFFSET 2 ROWS FETCH FIRST 10 ROWS ONLY;")
    case("missing-expression", "select 1 where;", kind="Unsupported",
         reason="Native requires the missing expression")
    case("syntax-missing-right-parenthesis", "select abs(1;", kind="Syntax",
         reason="an unfinished projection is a typed parse error")
    case("empty-sql-exact", "", kind="unchanged", reason="public API's documented exact blank preservation")
    case("whitespace-sql-exact", "\ufeff \r\n\t", kind="unchanged",
         reason="public API's documented exact blank preservation; oracle trim may differ")
    case("unsupported-space-width", "select 1;", tabWidth=3, kind="InvalidOptions",
         reason="public crate accepts only 2/4 spaces or tabs")
    case("unsupported-blank-line-count", "select 1;select 2;", linesBetweenQueries=17,
         kind="InvalidOptions", reason="public crate caps blank lines at 16")
    pad = "select 1;\n/*" + "x"*(256*1024-len("select 1;\n/**/"))+"*/"
    assert len(pad.encode()) == 256*1024
    case("input-exact-limit", pad, kind="exact", checked="SELECT\n  1;\n\n"+pad.split("\n",1)[1])
    case("input-one-byte-over", pad + "\n", kind="InputLimit",
         reason="typed public boundary 256 KiB + 1")
    large = "select " + "(" * 22 + ("1," * 40 + "\n") * 300 + "1" + ")" * 22 + ";"
    case("formatted-output-exceeds-cap", large, language="sqlite", kind="OutputLimit",
         reason="compact SQL expands into more than 512 KiB after indenting")
    assert len({row["id"] for row in result}) == len(result)
    return result


def run(binary, reference, dest, only):
    result = []
    rows = [r for r in collect_cases() if not only or r["id"] in only]
    assert rows and (not only or len(rows) == len(set(only)))
    dest.mkdir(parents=True, exist_ok=False)
    bundle_sha256 = json.loads((FIXTURES / "golden.json").read_text())["browserBundleSHA256"]
    # The original is an external comparison only. Do not ask it to
    # reproduce the Native API's explicit resource bounds.
    comparable = [r for r in rows if r["kind"] == "reference" or len(r["sql"].encode()) < 8192]
    oracle = json.loads(subprocess.run(
        ["node", str(ORACLE), str(reference), bundle_sha256],
        input=json.dumps(comparable), text=True, capture_output=True, timeout=60, check=True,
    ).stdout)
    for row in rows:
        id, sql, kind, options = row["id"], row["sql"], row["kind"], row["jsOptions"]
        command = [str(binary), "--dialect", options["language"], "--keywords", options["keywordCase"],
                   "--indent", "tabs" if options["useTabs"] else str(options["tabWidth"]),
                   "--boolean-break", options["logicalOperatorNewline"],
                   "--blank-lines", str(options["linesBetweenQueries"])]
        raw = dest/"raw"/f"{id}.profraw"
        raw.parent.mkdir(exist_ok=True)
        before = time.monotonic()
        env = dict(os.environ, LLVM_PROFILE_FILE=str(raw))
        got = subprocess.run(command, input=sql.encode(), env=env, capture_output=True, timeout=8)
        probe = oracle.get(id, {})
        ref = probe.get("output")
        if row["checked"] is not None and kind != "exact":
            assert ref == row["checked"], f"stored reference drift: {id}"
        if kind in ("reference", "unchanged", "exact"):
            expect = {"reference": ref, "unchanged": sql, "exact": row["checked"]}[kind]
            passed = (expect is not None and got.returncode == 0 and
                      got.stdout == expect.encode() and not got.stderr)
        else:
            passed = (got.returncode == 1 and not got.stdout and
                      got.stderr == (ERRORS[kind]+"\n").encode())
        record = dict(id=id, passed=passed, seconds=round(time.monotonic()-before,3),
                      profile=str(raw), observed_status=got.returncode, checked_as=kind,
                      oracle_result=("formatted" if ref is not None else "rejected") if probe else None,
                      dialect=options["language"],
                      byte_count=len(sql.encode()), sha_input=hashlib.sha256(sql.encode()).hexdigest(),
                      sha_stdout=hashlib.sha256(got.stdout).hexdigest(),
                      sha_expected=(hashlib.sha256(ref.encode()).hexdigest() if ref is not None else None),
                      reason=row["reason"], stderr=got.stderr.decode(errors="replace"))
        if passed:
            assert raw.is_file() and raw.stat().st_size > 0, id
        elif len(sql.encode()) < 2048:
            record["expected_stdout"] = ref
            record["observed_stdout"] = got.stdout.decode(errors="replace")
        print("PASS" if passed else "FAIL", id, record["observed_status"], f"{record['seconds']} s")
        result.append(record)
    (dest/"cases.json").write_text(json.dumps(rows, indent=2, ensure_ascii=False)+"\n")
    (dest/"results.json").write_text(json.dumps(result, indent=2, ensure_ascii=False)+"\n")
    (dest/"accepted-profile-list").write_text("".join(row["profile"]+"\n" for row in result if row["passed"]))
    return 0 if all(row["passed"] for row in result) else 1


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("binary", type=Path)
    p.add_argument("--reference", type=Path, required=True,
                   help="installed, pinned sql-formatter 15.8.2 package directory")
    p.add_argument("--output", type=Path, required=True, help="new results directory")
    p.add_argument("--only", action="append")
    args = p.parse_args()
    raise SystemExit(run(args.binary.resolve(strict=True), args.reference.resolve(strict=True),
                         args.output.resolve(), args.only))
