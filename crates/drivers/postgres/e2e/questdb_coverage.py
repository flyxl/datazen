#!/usr/bin/env python3
"""Run a reproducible QuestDB-only E2E line-coverage gate for the shared driver crate."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import tempfile
import time
from urllib.parse import urlencode
from urllib.request import ProxyHandler, build_opener

import coverage as gate

TEST = "questdb_live_e2e"


def loopback_ports():
    with ExitStack() as stack:
        sockets = [stack.enter_context(socket.socket()) for _ in range(2)]
        for listener in sockets:
            listener.bind(("127.0.0.1", 0))
        return [listener.getsockname()[1] for listener in sockets]


class QuestDB:
    def __init__(self, script, root, log, env):
        self.script, self.root, self.log, self.env = script, root, log, env
        self.pg, self.http = loopback_ports()
        self.password = secrets.token_hex(24)
        self.marker = secrets.token_hex(24)
        self.tag = "datazen-questdb-e2e-" + secrets.token_hex(8)
        self.opener = build_opener(ProxyHandler({}))
        self.log_file = None

    def query(self, query):
        url = f"http://127.0.0.1:{self.http}/exec?" + urlencode({"query": query})
        with self.opener.open(url, timeout=2) as response:
            result = json.load(response)
        if "error" in result:
            raise RuntimeError("QuestDB fixture initialization query failed")
        return result

    def invoke(self, action):
        gate.cmd([self.script, action, "-d", self.root, "-t", self.tag],
                 env=self.env, stdout=self.log_file, stderr=self.log_file,
                 timeout=40, check=True)

    def start(self):
        conf = self.root / "conf"
        conf.mkdir()
        (conf / "server.conf").write_text(
            f"pg.net.bind.to=127.0.0.1:{self.pg}\nhttp.bind.to=127.0.0.1:{self.http}\n"
            f"pg.password={self.password}\nhttp.min.enabled=false\n"
            "line.tcp.enabled=false\nline.udp.enabled=false\nshared.worker.count=2\n"
            "telemetry.enabled=false\n"
        )
        self.log_file = self.log.open("w")
        self.invoke("start")
        deadline = time.monotonic() + 25
        while True:
            try:
                build = self.query("SELECT build()")["dataset"][0][0]
                break
            except OSError:
                if time.monotonic() >= deadline:
                    raise RuntimeError("QuestDB HTTP readiness check timed out") from None
                time.sleep(0.25)
        if "QuestDB" not in build:
            raise RuntimeError("The fixture did not identify itself as QuestDB")
        self.query("CREATE TABLE datazen_qdb_marker (token STRING)")
        self.query(f"INSERT INTO datazen_qdb_marker VALUES ('{self.marker}')")
        return build

    def test_env(self):
        return {**self.env, "DATAZEN_TEST_QUESTDB_PORT": str(self.pg),
                "DATAZEN_TEST_QUESTDB_PASSWORD": self.password,
                "DATAZEN_TEST_QUESTDB_MARKER": self.marker}

    def stop(self):
        if self.log_file is not None:
            try:
                self.invoke("stop")
            finally:
                self.log_file.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--questdb-script", type=Path, required=True,
                        help="bin/questdb.sh from the official Linux runtime")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=gate.ROOT / "target/questdb-e2e")
    parser.add_argument("--report-only", action="store_true",
                        help="allow a sub-threshold report, never waive a failed or skipped test")
    args = parser.parse_args()
    script, output = args.questdb_script.resolve(strict=True), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("DATAZEN_TEST_QUESTDB_", "QDB_"))}
    source = {"commit": gate.read(["git", "rev-parse", "HEAD"]),
              "working_tree_dirty": bool(gate.read(["git", "status", "--porcelain"])),
              "rustc": gate.read(["rustc", "--version"])}
    # Link the PostgreSQL reference objects into the measurement to retain
    # the shared implementation in the denominator. Only QuestDB profiles run.
    binaries = gate.build({**gate.LEGACY, gate.JOURNEYS: None, TEST: None}, output,
                          env, args.target_dir.resolve())
    source["executables_sha256"] = {}
    for name, path in binaries.items():
        with path.open("rb") as stream:
            source["executables_sha256"][name] = hashlib.file_digest(stream, "sha256").hexdigest()
    with tempfile.TemporaryDirectory(prefix="questdb-", dir=output) as temp:
        fixture = QuestDB(script, Path(temp), output / "server.log", env)
        try:
            source["questdb"] = fixture.start()
            counts, profiles = gate.run_live({TEST: binaries[TEST]}, {TEST: None},
                                             fixture.test_env(), fixture.password, output)
        finally:
            fixture.stop()
    paths, hit, total = gate.measure(binaries, profiles, output, gate.llvm_tools())
    report = {"source": source, "profile_engine": "questdb", "scope": str(gate.SCOPE),
              "files": paths, "covered": hit, "total": total, "percent": 100 * hit / total,
              "live_e2e": counts, "minimum_percent": gate.TARGET,
              "meets_threshold": hit * 100 >= total * gate.TARGET}
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"QuestDB-only driver E2E lines: {hit}/{total} ({report['percent']:.2f}%); "
          f"required {gate.TARGET}%")
    return int(bool(counts["failed"] or counts["skipped"])
               or not (args.report_only or report["meets_threshold"]))


if __name__ == "__main__":
    raise SystemExit(main())
