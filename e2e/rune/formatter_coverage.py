#!/usr/bin/env python3
"""Run only rendered formatter journeys; verify the supplied Linux LLVM build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from coverage import llvm_tools

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CRATE = ROOT / "crates" / "datazen-sql-formatter" / "src"


def digest(path):
    with path.open("rb") as fd:
        return hashlib.file_digest(fd, "sha256").hexdigest()


def summarize(binary, output, profdata, cov, raws):
    # Never filter filenames down to the desired crate. An unexpected
    # instrumented crate (or test module) invalidates this measurement.
    merged = output / "ui-only.profdata"
    subprocess.run([profdata, "merge", "-sparse", *raws, "-o", merged], check=True)
    result = subprocess.check_output([cov, "export", binary, "-summary-only",
                                      f"-instr-profile={merged}"])
    report = json.loads(result)
    if len(report["data"]) != 1:
        raise ValueError("unexpected LLVM dataset count")
    dataset = report["data"][0]
    if not dataset["files"]:
        raise ValueError("no formatter coverage mapping")
    lines = {}
    for f in dataset["files"]:
        mapped = Path(f["filename"])
        # llvm-cov emits absolute compilation paths when the build was made
        # in a different checkout. Require the same local production source.
        parts = mapped.parts
        prefix = ("crates", "datazen-sql-formatter", "src")
        offsets = [i for i in range(len(parts)) if parts[i:i + 3] == prefix]
        if len(offsets) != 1 or ".." in parts:
            raise ValueError(f"unexpected compiler coverage input {mapped.name}")
        rel = Path(*parts[offsets[0] + 3:])
        src = CRATE / rel
        if rel in lines or src.suffix != ".rs" or not src.is_file() or not mapped.is_file() or digest(mapped) != digest(src):
            raise ValueError(f"production source mismatch for {rel}")
        m = f["summary"]["lines"]
        lines[rel] = (m["covered"], m["count"])
    sums = tuple(map(sum, zip(*lines.values())))
    total = dataset["totals"]["lines"]
    if sums != (total["covered"], total["count"]) or not total["count"]:
        raise ValueError("file/total line counts disagree")
    (output / "llvm-summary.json").write_bytes(result)
    with (output / "llvm-report.txt").open("w") as fd:
        subprocess.run([cov, "report", binary, f"-instr-profile={merged}"], stdout=fd, check=True)
    with (output / "mapped-lines.txt").open("w") as fd:
        subprocess.run([cov, "show", binary, f"-instr-profile={merged}"], stdout=fd, check=True)
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="stock binary with only datazen_sql_formatter instrumented")
    parser.add_argument("--native-only", action="store_true", help="report Native-only coverage without a mixed gate")
    parser.add_argument("--formatter-rlib", type=Path, help="exact instrumented production rlib from this Native build")
    parser.add_argument("--reference", type=Path, help="installed pinned sql-formatter 15.8.2 oracle package")
    parser.add_argument("--output", type=Path, required=True, help="new directory (must not already exist)")
    for engine in ("postgresql", "mysql", "mariadb"):
        parser.add_argument(f"--{engine}-fixture", type=Path, required=True)
    for engine in ("mysql", "mariadb"):
        parser.add_argument(f"--{engine}-log", type=Path, required=True,
                            help="live, disposable server general query log")
    parser.add_argument("--report-only", action="store_true", help="allow <99%%; never allow a failed journey")
    args = parser.parse_args()
    if not args.native_only and not (args.formatter_rlib and args.reference):
        parser.error("the default mixed 99% gate requires --formatter-rlib and --reference")
    profdata, cov = llvm_tools()
    binary = args.binary.resolve(strict=True)
    initial = digest(binary)
    out = args.output.resolve()
    out.mkdir(mode=0o700)
    passes = []
    for group in ((), ("--definitions",), ("--statements",), ("--size-bound",), ("--selection-repair",)):
        passes.append(("sql_formatter.py", group))
    for engine in ("sqlite", "postgresql", "mysql", "mariadb"):
        fixture = () if engine == "sqlite" else ("--fixture", str(getattr(args, engine + "_fixture").resolve(strict=True)))
        passes.append(("sql_formatter.py", ("--dialect", engine, *fixture)))
        if engine in ("sqlite", "postgresql"):
            passes.append(("sql_formatter_refusals.py", (engine, *fixture)))
        else:
            log = str(getattr(args, engine + "_log").resolve(strict=True))
            passes.append(("sql_formatter_mysql_refusals.py", (engine, *fixture, "--server-log", log)))
    passes.append(("sql_formatter_refusals.py", ("sqlite", "--output-bound")))
    raws, receipts = [], []
    for i, (script, flags) in enumerate(passes):
        folder = out / f"group-{i + 1:02d}"
        raw, trees = folder / "raw", folder / "trees"
        raw.mkdir(parents=True)
        env = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1", "PYTHONOPTIMIZE": "0",
               "LLVM_PROFILE_FILE": str(raw / "%p.profraw")}
        cmd = [sys.executable, "-B", HERE / script, binary, *flags, "--artifacts", trees]
        result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)
        (folder / "tests.log").write_text(result.stdout, encoding="utf-8")
        print(result.stdout, end="", flush=True)
        found = re.search(r"^Ran (\d+) tests? in [\d.]+s$", result.stdout, re.M)
        if result.returncode or not found or not result.stdout.rstrip().endswith("\nOK"):
            raise SystemExit(f"{script} group {i + 1} failed, skipped or did not report success")
        count = int(found[1])
        group_raw = sorted(raw.glob("*.profraw"))
        n = len(group_raw)
        if not count or n < count or n != len(list(trees.glob("*.tree.json"))) or not all(p.stat().st_size for p in group_raw):
            raise SystemExit(f"group {i + 1}: expected one nonempty raw profile for each actual render")
        raws.extend(group_raw)
        receipts.append({"runner": script, "tests": count, "renders": n})
    if digest(binary) != initial:
        raise SystemExit("tested binary changed during replay")
    total = summarize(binary, out, profdata, cov, raws)
    summary = {"binary_sha256": initial, "groups": receipts, "profiles": len(raws), "lines": total}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Formatter E2E: {total['covered']}/{total['count']} production lines, {total['percent']:.2f}%")
    if args.native_only:
        return 0
    from formatter_coverage_mixed import mixed_report
    reports = mixed_report(CRATE.parent / "e2e", out, binary, raws,
                           args.formatter_rlib.resolve(strict=True), args.reference.resolve(strict=True),
                           profdata, cov, summarize, digest)
    if digest(binary) != initial:
        raise SystemExit("Native executable changed during measurement")
    for label, row in reports.items():
        print(f"{label}: {row['covered']}/{row['count']} ({row['percent']:.2f}%)")
    total = reports["combined"]
    return 0 if args.report_only or total["covered"] * 100 >= total["count"] * 99 else 1


if __name__ == "__main__":
    raise SystemExit(main())
