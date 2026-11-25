#!/usr/bin/env python3
"""Provision a private Redis server and exercise the driver's public boundary."""
import argparse
import base64
import contextlib
import copy
import json
import os
import pickle
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile
import time

from rune_tools_common import Lab


def write_pickle_fixtures(root: Path):
    label = "数据"
    nested = {"items": [label, None, 256, -123456, 2.25, label], "tuple": (True, False, 4)}
    cases = [(pickle.dumps(nested, protocol=p), nested) for p in (0, 1, 2, 3, 4, 5)]
    text = "latin café; 数据; 🌍; backslash \\n; newline \n."
    cases.append((pickle.dumps(text, protocol=0), text))
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
    parser.add_argument("--native-harness", action="store_true")
    parser.add_argument("--test-json", action="store_true")
    parser.add_argument("--test-topology", action="store_true")
    parser.add_argument("--legacy-server", type=Path, help="optional Redis 5 executable for KEEPTTL compatibility")
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
        options = ["--databases", "8"]
        if module:
            options.extend(["--loadmodule", str(module.resolve(strict=True))])
        lab.start(port, options)
        nonce = secrets.token_hex(16)
        assert lab.cli(port, "SET", "contract:fixture", nonce) == "OK"
        if args.test_topology:
            sentinel = lab.port()
            lab.start(sentinel, sentinel=port)
            cluster = lab.port(cluster=True)
            lab.start(cluster, ["--cluster-enabled", "yes", "--cluster-config-file", "nodes.conf",
                                "--cluster-node-timeout", "1000", "--cluster-announce-ip", "127.0.0.1"])
            assert lab.cli(cluster, "CLUSTER", "ADDSLOTSRANGE", "0", "16383") == "OK"
            deadline = time.monotonic() + 8
            while "cluster_state:ok" not in lab.cli(cluster, "CLUSTER", "INFO"):
                assert time.monotonic() < deadline, "fixture cluster did not converge"
                time.sleep(.15)
            assert lab.cli(cluster, "SET", "contract:fixture", nonce) == "OK"
            lab.env.update(REDIS_E2E_SENTINEL_PORT=str(sentinel), REDIS_E2E_CLUSTER_PORT=str(cluster))
        if args.legacy_server:
            legacy_args = copy.copy(args)
            legacy_args.server = str(args.legacy_server.resolve(strict=True))
            legacy = Lab(legacy_args, stack, root)
            legacy_port = legacy.port()
            legacy.start(legacy_port)
            assert legacy.cli(legacy_port, "SET", "contract:fixture", nonce) == "OK"
            lab.env["REDIS_E2E_LEGACY_PORT"] = str(legacy_port)
        fixture_args = ["--redis-driver-fixture"] if args.native_harness else []
        subprocess.run([str(args.binary.resolve(strict=True)), *fixture_args, str(port), nonce, *(["json"] if args.test_json else [])],
                       cwd=root, env=lab.env, timeout=60, check=True)


if __name__ == "__main__":
    main()
