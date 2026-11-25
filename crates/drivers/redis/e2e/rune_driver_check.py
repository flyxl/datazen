#!/usr/bin/env python3
"""Provision a private Redis server and exercise the driver's public boundary."""
import argparse
import contextlib
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile

from rune_tools_common import Lab


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--server", default=shutil.which("redis-server"))
    parser.add_argument("--cli", default=shutil.which("redis-cli"))
    parser.add_argument("--test-json", action="store_true")
    parser.add_argument("--redis-json-module", type=Path)
    args = parser.parse_args()
    if not args.server or not args.cli:
        parser.error("Redis server and CLI executables are required.")
    args.server, args.cli = os.path.abspath(args.server), os.path.abspath(args.cli)
    with contextlib.ExitStack() as stack:
        root = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix="rune-redis-driver-")))
        lab = Lab(args, stack, root)
        port = lab.port()
        module = args.redis_json_module
        lab.start(port, ["--loadmodule", str(module.resolve(strict=True))] if module else [])
        nonce = secrets.token_hex(16)
        assert lab.cli(port, "SET", "contract:fixture", nonce) == "OK"
        subprocess.run([str(args.binary.resolve(strict=True)), str(port), nonce, *(["json"] if args.test_json else [])],
                       cwd=root, env=lab.env, timeout=60, check=True)


if __name__ == "__main__":
    main()
