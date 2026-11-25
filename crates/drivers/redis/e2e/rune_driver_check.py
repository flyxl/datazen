#!/usr/bin/env python3
"""Provision a private Redis server and exercise the driver's public boundary."""
import argparse
import base64
import contextlib
import json
import os
import pickle
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile

from rune_tools_common import Lab


def write_pickle_fixtures(root: Path):
    label = "数据"
    nested = {"items": [label, None, 256, -123456, 2.25, label], "tuple": (True, False, 4)}
    cases = [(pickle.dumps(nested, protocol=p), nested) for p in (2, 3, 4, 5)]
    binary = bytes(range(256))
    for protocol in (3, 4, 5):
        cases.append((pickle.dumps(binary, protocol=protocol),
                      base64.b64encode(binary).decode("ascii")))
    for scalar in (123, -45, 2.5, 10**30):
        expected = str(scalar) if scalar == 10**30 else scalar
        cases.append((pickle.dumps(scalar, protocol=0), expected))
    (root / "pickle-fixtures.json").write_text(json.dumps([
        {"data": base64.b64encode(payload).decode("ascii"), "expected": expected}
        for payload, expected in cases
    ]), encoding="utf-8")


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
        write_pickle_fixtures(root)
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
