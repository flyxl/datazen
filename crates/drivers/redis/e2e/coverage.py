#!/usr/bin/env python3
"""Measure driver-crate line coverage from real native Redis journeys only."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
SCOPE = ROOT / "crates" / "drivers" / "redis" / "src"
THRESHOLD = 90
SUITES = (
    "standalone", "tools", "topology", "lifecycle", "pubsub", "search", "tls",
    "inspection", "redis-json", "additions", "form", "stream-groups", "batch-delete",
    "cluster-multishard", "key-filters", "console-batch", "removals", "probe",
    "binary-view", "layout", "console-completion", "console-set", "server-folders", "driver-contract",
)


def llvm_tools():
    sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    version = subprocess.check_output(["rustc", "-vV"], text=True)
    triple = next(line.split()[1] for line in version.splitlines() if line.startswith("host: "))
    tools = [Path(sysroot) / "lib" / "rustlib" / triple / "bin" / name
             for name in ("llvm-profdata", "llvm-cov")]
    if not all(tool.is_file() for tool in tools):
        raise SystemExit("Install LLVM tools with: rustup component add llvm-tools-preview")
    return tools


def summarize(payload, results, output: Path):
    files = {}
    for dataset in payload["data"]:
        for entry in dataset["files"]:
            path = (ROOT / entry["filename"]).resolve()
            if path.is_relative_to(SCOPE):
                counts = entry["summary"]["lines"]
                files[str(path.relative_to(ROOT))] = {
                    "covered": counts["covered"],
                    "total": counts["count"],
                }
    covered = sum(row["covered"] for row in files.values())
    total = sum(row["total"] for row in files.values())
    if not total:
        raise SystemExit("No Redis production source lines found in the instrumented binary.")
    percent = covered * 100 / total
    success = all(result == 0 for result in results.values())
    meets_target = covered * 100 > THRESHOLD * total
    report = {
        "scope": "crates/drivers/redis/src",
        "suite": "native and public-driver real-Redis E2E only",
        "line_coverage_percent": percent,
        "covered_lines": covered,
        "total_lines": total,
        "threshold_percent": THRESHOLD,
        "strictly_above_threshold": meets_target,
        "all_journeys_passed": success,
        "journey_exit_codes": results,
        "files": files,
    }
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Redis crate E2E line coverage: {covered}/{total} ({percent:.2f}%); required: >{THRESHOLD}%",
          flush=True)
    return success and meets_target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target" / "redis-e2e-coverage")
    parser.add_argument("--server", default=shutil.which("redis-server"))
    parser.add_argument("--cli", default=shutil.which("redis-cli"))
    parser.add_argument("--redis-json-module", type=Path)
    parser.add_argument("--json-server", help="optional separate Redis server matching the JSON module")
    parser.add_argument("--jobs", default=1, type=int)
    args = parser.parse_args()
    if not args.server or not args.cli:
        parser.error("Redis server and CLI are required; specify --server and --cli if not on PATH.")
    if args.jobs < 1:
        parser.error("--jobs must be positive")

    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "summary.json").unlink(missing_ok=True)
    target = args.target_dir.resolve()
    binary = target / "debug" / ("datazen-native.exe" if os.name == "nt" else "datazen-native")
    driver_binary = target / "debug" / "examples" / ("redis_driver_e2e.exe" if os.name == "nt" else "redis_driver_e2e")
    profdata, cov = llvm_tools()
    env = {
        **os.environ,
        "CARGO_TARGET_DIR": str(target),
        "CARGO_INCREMENTAL": "0",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "RUSTFLAGS": "-C instrument-coverage",
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    with tempfile.TemporaryDirectory(prefix="redis-cov-build-") as temp:
        env["LLVM_PROFILE_FILE"] = str(Path(temp) / "%p-%m.profraw")
        subprocess.run(["cargo", "build", "--locked", "-p", "datazen-rune-host",
                        "--bin", "datazen-native"], env=env, cwd=ROOT, check=True)
        subprocess.run(["cargo", "build", "--locked", "-p", "datazen-driver-redis",
                        "--example", "redis_driver_e2e"], env=env, cwd=ROOT, check=True)

    with tempfile.TemporaryDirectory(prefix="redis-cov-e2e-") as temp:
        profiles = Path(temp)
        env["LLVM_PROFILE_FILE"] = str(profiles / "%p-%m.profraw")

        def replay(suite):
            snapshots = output / "trees" / suite
            if suite == "driver-contract":
                argv = [sys.executable, str(HERE / "rune_driver_check.py"), str(driver_binary),
                        "--server", args.json_server or args.server, "--cli", args.cli, "--test-json"]
                if args.redis_json_module:
                    argv.extend(["--redis-json-module", str(args.redis_json_module.resolve())])
            elif suite == "standalone":
                argv = [sys.executable, str(HERE / "rune_standalone.py"), str(binary),
                        "--redis-server", args.server, "--redis-cli", args.cli]
            else:
                server = args.json_server if suite == "redis-json" and args.json_server else args.server
                argv = [sys.executable, str(HERE / "rune_tools.py"), str(binary), "--only", suite,
                        "--server", server, "--cli", args.cli]
                if suite == "redis-json" and args.redis_json_module:
                    argv.extend(["--redis-json-module", str(args.redis_json_module.resolve())])
            with (output / f"{suite}.log").open("w", encoding="utf-8") as log:
                artifact_args = [] if suite == "driver-contract" else ["--artifacts", str(snapshots)]
                run = subprocess.run([*argv, *artifact_args], cwd=ROOT,
                                     env=env, stdout=log, stderr=subprocess.STDOUT)
            print(f"{suite}: {'pass' if run.returncode == 0 else 'fail'}", flush=True)
            return suite, run.returncode

        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            results = dict(pool.map(replay, SUITES))
        raw = sorted(profiles.glob("*.profraw"))
        if not raw:
            raise SystemExit("The journeys produced no application coverage profiles.")
        merged = output / "redis-e2e.profdata"
        subprocess.run([profdata, "merge", "-sparse", "-o", merged, *raw], check=True)
        with (output / "llvm-summary.json").open("w", encoding="utf-8") as report:
            subprocess.run([cov, "export", "-summary-only", f"-instr-profile={merged}", binary,
                            "-object", driver_binary],
                           stdout=report, check=True)
    return 0 if summarize(json.loads((output / "llvm-summary.json").read_text()), results, output) else 1


if __name__ == "__main__":
    sys.exit(main())
