"""Opt-in Redis UI journeys, using the ordinary native release entry point."""
import os
from pathlib import Path
import subprocess
import sys

TOOLS = Path(__file__).resolve().parents[2] / "crates" / "drivers" / "redis" / "e2e" / "rune_tools.py"
REDIS_UI_CASES = ("inspection", "lifecycle", "pubsub", "removals", "stream-groups",
                  "console-completion", "server-folders", "key-filters")


def configure_redis(parser, args):
    if bool(args.redis_server) != bool(args.redis_cli):
        parser.error("--redis-server and --redis-cli must be supplied together")
    if not args.redis_server:
        return None
    paths = tuple(path.absolute() for path in (args.redis_server, args.redis_cli))
    if not all(path.is_file() and os.access(path, os.X_OK) for path in paths):
        parser.error("Redis server and CLI must be executable files")
    return paths


def add_redis_arguments(parser):
    parser.add_argument("--redis-server", type=Path)
    parser.add_argument("--redis-cli", type=Path)


def register_redis_ui_cases(suite, binary, redis_fixture, artifacts):
    if not redis_fixture:
        return
    server, cli = redis_fixture

    def run_case(self, case):
        args = [sys.executable, str(TOOLS), str(binary), "--server", str(server),
                "--cli", str(cli), "--only", case]
        if artifacts:
            args.extend(("--artifacts", str(artifacts / f"redis-{case}")))
        env = os.environ.copy()
        profile = env.get("LLVM_PROFILE_FILE")
        output_pattern = None
        if profile:
            pattern = Path(profile).absolute()
            output_pattern = pattern.with_name(f"redis-{case}-{pattern.name}")
            env["LLVM_PROFILE_FILE"] = str(output_pattern)
            existing = set(output_pattern.parent.glob(f"redis-{case}-*.profraw"))
        result = subprocess.run(args, text=True, capture_output=True, env=env, timeout=600)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(any(line.startswith("PASS: ") for line in result.stdout.splitlines()),
                        "The Redis UI fixture did not confirm execution")
        if output_pattern is not None:
            produced = set(output_pattern.parent.glob(f"redis-{case}-*.profraw")) - existing
            self.assertTrue(any(path.stat().st_size for path in produced),
                            "The Redis UI fixture did not produce native coverage profiles")

    for case in REDIS_UI_CASES:
        def journey(self, case=case):
            run_case(self, case)
        setattr(suite, f"test_redis_native_{case.replace('-', '_')}", journey)
