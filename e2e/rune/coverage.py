#!/usr/bin/env python3
"""Build the native app with LLVM instrumentation and report E2E-only line coverage."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from redis_replay import add_redis_arguments, configure_redis

ROOT = Path(__file__).resolve().parents[2]
SCOPE = ("datazen-ui", "datazen-ui-api", "datazen-rune-host")
MINIMUM = 90


def run(*args, env=None, stdout=None):
    return subprocess.run(args, cwd=ROOT, env=env, stdout=stdout, check=True)


def llvm_tools():
    sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    verbose = subprocess.check_output(["rustc", "-vV"], text=True)
    host = next(line.split()[1] for line in verbose.splitlines() if line.startswith("host: "))
    base = Path(sysroot) / "lib" / "rustlib" / host / "bin"
    tools = tuple(base / name for name in ("llvm-profdata", "llvm-cov"))
    if not all(tool.is_file() for tool in tools):
        raise SystemExit("Missing LLVM tools: run rustup component add llvm-tools-preview")
    return tools


def report(raw, output, journeys, source):
    files = {}
    for dataset in raw["data"]:
        for entry in dataset["files"]:
            path = Path(entry["filename"])
            if ".." in path.parts:
                continue  # Instrumentation can map fixture/test source via src/../tests.
            path = (ROOT / path).resolve()
            for crate in SCOPE:
                if path.is_relative_to(ROOT / "crates" / crate / "src"):
                    lines = entry["summary"]["lines"]
                    files[str(path.relative_to(ROOT))] = (lines["covered"], lines["count"])
    by_crate = {}
    for crate in SCOPE:
        paths = [counts for path, counts in files.items() if path.startswith(f"crates/{crate}/src/")]
        hit, total = sum(pair[0] for pair in paths), sum(pair[1] for pair in paths)
        if not total:
            raise SystemExit(f"No source lines in coverage mapping for {crate}; cannot verify coverage")
        by_crate[crate] = {"covered": hit, "total": total, "percent": 100 * hit / total}
    hit = sum(row["covered"] for row in by_crate.values())
    total = sum(row["total"] for row in by_crate.values())
    summary = {"source": source, "crates": by_crate, "covered": hit, "total": total, "percent": 100 * hit / total,
               "minimum_percent": MINIMUM, "meets_threshold": hit * 100 >= total * MINIMUM,
               "journeys": journeys, "files": files}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    for crate, counts in by_crate.items():
        print(f"{crate}: {counts['covered']}/{counts['total']} ({counts['percent']:.2f}%)")
    print(f"E2E line coverage: {hit}/{total} ({100 * hit / total:.2f}%); required: {MINIMUM}%")
    return hit * 100 >= total * MINIMUM


def run_journeys(binary, output, env, jobs, servers, redis_fixture):
    runner = [sys.executable, str(ROOT / "e2e/rune/run.py"), str(binary),
              "--artifacts", str(output / "trees")]
    for engine, config in servers.items():
        runner.extend((f"--{engine}-config", str(config)))
    if redis_fixture:
        runner.extend(("--redis-server", str(redis_fixture[0]), "--redis-cli", str(redis_fixture[1])))
    names = json.loads(subprocess.check_output([*runner, "--list-tests"], cwd=ROOT, env=env))
    if not names:
        raise SystemExit("No native E2E journeys discovered")
    jobs = min(jobs, len(names))

    def shard(index):
        selected = names[index::jobs]
        status = output / f"journeys-{index + 1}.json"
        status.unlink(missing_ok=True)
        result = subprocess.run([*runner, "--status-json", str(status), *selected],
                                cwd=ROOT, env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        counts = json.loads(status.read_text()) if status.exists() else {}
        return index, result, counts

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(shard, range(jobs)))
    for index, result, _ in results:
        print(f"===== Native E2E shard {index + 1}/{jobs} =====")
        print(result.stdout, end="")
        (output / f"journeys-{index + 1}.log").write_text(result.stdout, encoding="utf-8")
    counts = {key: sum(c.get(key, 0) for _, _, c in results)
              for key in ("run", "passed", "failed", "skipped")}
    return {**counts, "discovered": len(names),
            "all_passed": counts["passed"] == len(names) and all(
                result.returncode == 0 for _, result, _ in results
            )}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target" / "rune-e2e-coverage")
    parser.add_argument("--jobs", default=1, type=int, help="number of independent test shards")
    parser.add_argument("--postgres-config", type=Path)
    parser.add_argument("--mysql-config", type=Path)
    parser.add_argument("--report-only", action="store_true",
                        help="measure coverage without enforcing 90%%; still fail on E2E or LLVM errors")
    add_redis_arguments(parser)
    args = parser.parse_args()
    redis_fixture = configure_redis(parser, args)
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    servers = {engine: config.resolve(strict=True)
               for engine, config in (("postgres", args.postgres_config), ("mysql", args.mysql_config))
               if config is not None}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "summary.json").unlink(missing_ok=True)
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

    source = {"commit": git("rev-parse", "HEAD"),
              "working_tree_dirty": bool(git("status", "--porcelain"))}
    profdata, cov = llvm_tools()
    target = args.target_dir.resolve()
    built_binary = target / "debug" / "datazen-native"
    env = {**os.environ, "CARGO_TARGET_DIR": str(target), "CARGO_INCREMENTAL": "0",
           "RUSTFLAGS": "-C instrument-coverage", "CARGO_PROFILE_DEV_DEBUG": "0"}
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    with tempfile.TemporaryDirectory(prefix="build-prof-", dir=output) as directory:
        # Cargo also runs instrumented build scripts. Never merge those profiles.
        env["LLVM_PROFILE_FILE"] = str(Path(directory) / "%p-%m.profraw")
        run("cargo", "build", "--locked", "-p", "datazen-rune-host", "--bin", "datazen-native", env=env)

    with tempfile.TemporaryDirectory(prefix="e2e-prof-", dir=output) as directory:
        profiles = Path(directory)
        # Freeze the executable used for both replay and export. Developing or
        # compiling the same worktree during replay cannot mix profraw mappings.
        binary = profiles / "datazen-native"
        shutil.copy2(built_binary, binary)
        with binary.open("rb") as source_binary:
            source["binary_sha256"] = hashlib.file_digest(source_binary, "sha256").hexdigest()
        source["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip()
        env["LLVM_PROFILE_FILE"] = str(profiles / "%p-%m.profraw")
        journeys = run_journeys(binary, output, env, args.jobs, servers, redis_fixture)
        journeys["server_engines"] = sorted([*servers, *(["redis"] if redis_fixture else [])])
        raw = sorted(profiles.glob("*.profraw"))
        if not raw:
            raise SystemExit("The instrumented executable produced no coverage profiles")
        merged = output / "e2e.profdata"
        run(profdata, "merge", "-sparse", "-o", merged, *raw)
        def export(name, *options):
            with (output / name).open("w", encoding="utf-8") as stream:
                result = subprocess.run(
                    [cov, "export", *options, f"-instr-profile={merged}", binary],
                    cwd=ROOT, stdout=stream, stderr=subprocess.PIPE, text=True, check=True,
                )
            if result.stderr.strip():
                raise SystemExit(f"LLVM did not export a clean report: {result.stderr.strip()}")

        export("llvm-summary.json", "-summary-only")
        with tempfile.TemporaryDirectory(prefix="lcov-", dir=output) as directory:
            raw_lcov = Path(directory) / "all.lcov"
            export(raw_lcov, "-format=lcov")
            with raw_lcov.open(encoding="utf-8") as lcov_source, (output / "rune-e2e.lcov").open(
                "w", encoding="utf-8"
            ) as scoped:
                keep = False
                for line in lcov_source:
                    if line.startswith("SF:"):
                        path = ROOT / line[3:].rstrip("\n")
                        resolved = path.resolve()
                        keep = ".." not in path.parts and any(
                            resolved.is_relative_to(ROOT / "crates" / crate / "src")
                            for crate in SCOPE
                        )
                        if keep:
                            line = f"SF:{resolved.relative_to(ROOT)}\n"
                    if keep:
                        scoped.write(line)
                    if line == "end_of_record\n":
                        keep = False
        hit_target = report(json.loads((output / "llvm-summary.json").read_text()), output, journeys, source)
        return 0 if journeys["all_passed"] and (hit_target or args.report_only) else 1


if __name__ == "__main__":
    sys.exit(main())
