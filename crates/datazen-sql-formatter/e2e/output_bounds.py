#!/usr/bin/env python3
"""External process tests of bounded Native formatter output and token behavior."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
LIMIT_OUT = 512 * 1024
DEPTH = 22
DEPTH_INDENT = 2
NEWLINE = "\n"
ERRORS = {
    "output": b"SQL format output limit exceeded\n",
    "tokens": b"SQL format input limit exceeded\n",
}


def seed(num, tail):
    return "select " + "(" * DEPTH + "1," * (num - 1) + tail + ")" * DEPTH + ";"


def formatted_table(num, tail):
    # Structure independently verified against the pinned SQL formatter on
    # a small draft: each sufficiently wide group gets one item per line.
    rows = ["SELECT"]
    rows.extend(" " * (DEPTH_INDENT * level) + "(" for level in range(1, DEPTH + 1))
    column = " " * (DEPTH_INDENT * (DEPTH + 1))
    rows.extend(column + "1," for _ in range(num - 1))
    rows.append(column + tail)
    rows.extend(" " * (DEPTH_INDENT * level) + ")" for level in range(DEPTH, 0, -1))
    return NEWLINE.join(rows) + ";"


def cases():
    yield "small-shape-cjs-and-browser", seed(100, "1"), formatted_table(100, "1"), None
    step = len(formatted_table(101, "1")) - len(formatted_table(100, "1"))
    overhead = len(formatted_table(100, "1")) - step * 100
    items, remainder = divmod(LIMIT_OUT - overhead, step)
    at_tail = "7" * (remainder + 1)
    assert len(formatted_table(items, at_tail)) == LIMIT_OUT
    yield "output-exact", seed(items, at_tail), formatted_table(items, at_tail), None
    yield "output-one-over", seed(items, at_tail + "7"), None, "output"
    yield "newline-gap-beyond-output", seed(items, at_tail) + " select 1;", None, "output"

    first, second = 6000, 6001
    assert len(formatted_table(first, "1")) < LIMIT_OUT
    assert len(formatted_table(second, "1")) < LIMIT_OUT
    assert len(formatted_table(first, "1") + "\n\n" + formatted_table(second, "1")) > LIMIT_OUT + 4096
    yield "cumulative-two-statements", seed(first, "1") + "\n" + seed(second, "1"), None, "output"

    # Under the byte cap but after a distinctly excessive token count.
    oversized_list = "select " + "1," * 32768 + "1;"
    assert len(oversized_list.encode()) < 256 * 1024
    yield "very-wide-list-token-bound", oversized_list, None, "tokens"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--reference", type=Path, required=True,
                        help="installed, pinned sql-formatter 15.8.2 package directory")
    parser.add_argument("--output", type=Path, required=True, help="new results directory")
    args = parser.parse_args()
    dest = args.output.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    exe = args.binary.resolve(strict=True)
    oracle = ROOT / "reference_oracle.cjs"
    reference = args.reference.resolve(strict=True)
    golden = ROOT.parent / "tests" / "reference" / "golden.json"
    bundle_sha256 = json.loads(golden.read_text())["browserBundleSHA256"]
    accepted = []
    for name, draft, exact, error in cases():
        encoded = draft.encode()
        if name == "small-shape-cjs-and-browser":
            options = dict(language="sqlite", keywordCase="upper", tabWidth=2,
                           useTabs=False, logicalOperatorNewline="before", linesBetweenQueries=1)
            probe = [dict(id=name, sql=draft, jsOptions=options)]
            pinned = subprocess.run(["node", str(oracle), str(reference), bundle_sha256], input=json.dumps(probe), text=True,
                                    capture_output=True, timeout=30, check=True)
            assert json.loads(pinned.stdout)[name]["output"] == exact
        profile = dest / f"{name}.profraw"
        result = subprocess.run([exe, "--dialect", "sqlite"], input=encoded, capture_output=True,
                                env={**os.environ,"LLVM_PROFILE_FILE":str(profile)}, timeout=8)
        if exact is not None:
            assert result.returncode == 0 and not result.stderr and result.stdout == exact.encode(), (
                name, result.returncode, len(result.stdout), result.stderr)
        else:
            assert result.returncode == 1 and not result.stdout and result.stderr == ERRORS[error], (
                name, result.returncode, len(result.stdout), result.stderr)
        assert profile.is_file() and profile.stat().st_size
        accepted.append(dict(case=name, status=result.returncode, bytes_in=len(encoded),
                             bytes_out=len(result.stdout), input_sha256=hashlib.sha256(encoded).hexdigest(),
                             output_sha256=hashlib.sha256(result.stdout).hexdigest(),
                             diagnostic=result.stderr.decode(), profile=profile.name))
        print("PASS", name, result.returncode, len(encoded), len(result.stdout))
    (dest / "passing.json").write_text(json.dumps(accepted, indent=2)+"\n")
    (dest / "accepted-profile-list").write_text("".join(str(dest/r["profile"])+"\n" for r in accepted))


if __name__ == "__main__":
    main()
