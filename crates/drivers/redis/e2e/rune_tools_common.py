"""Shared fixtures for native Redis tool journeys."""
import json
import os
from pathlib import Path
import socket
import subprocess
import time

from rune_standalone import element, tap


def enter(t, key, value):
    return [tap(t, key), {"t": t + .2, "type": "key", "name": "a", "cmd": True},
            {"t": t + .4, "type": "type", "text": value, "every": .025}]


class Lab:
    def __init__(self, args, stack, root):
        self.args, self.stack, self.root = args, stack, root
        self.counter = 0
        self.env = {**os.environ, "DATAZEN_NATIVE_CONNECTIONS": str(root / "connections.json"),
                    "DATAZEN_NATIVE_KEYRING": "file", "XDG_DATA_HOME": str(root / "data"),
                    "XDG_CONFIG_HOME": str(root / "config"), "XDG_CACHE_HOME": str(root / "cache")}

    def cli(self, port, *command, db=0, ca=None):
        return subprocess.check_output(
            [self.args.cli, "-h", "127.0.0.1", "-p", str(port), "-n", str(db),
             *(["--tls", "--cacert", str(ca)] if ca is not None else []), "--raw", *command],
            text=True, stderr=subprocess.STDOUT, timeout=8,
        ).strip()

    def port(self, cluster=False):
        if not cluster:
            with socket.socket() as probe:
                probe.bind(("127.0.0.1", 0))
                return probe.getsockname()[1]
        # Cluster's bus uses the data port + 10000.
        for port in range(23480, 23680):
            with socket.socket() as data, socket.socket() as bus:
                try:
                    data.bind(("127.0.0.1", port))
                    bus.bind(("127.0.0.1", port + 10000))
                except OSError:
                    continue
                return port
        raise RuntimeError("No local Redis cluster port pair available")

    def start(self, port, extra=(), sentinel=None, ca=None):
        directory = self.root / f"redis-{port}"
        directory.mkdir()
        argv = [self.args.server]
        if sentinel is not None:
            config = directory / "sentinel.conf"
            config.write_text(
                f"port {port}\nbind 127.0.0.1\nprotected-mode yes\n"
                f"sentinel monitor native-test 127.0.0.1 {sentinel} 1\n",
                encoding="utf-8",
            )
            argv.extend([str(config), "--sentinel"])
        else:
            argv.extend(["--bind", "127.0.0.1", "--protected-mode", "yes", "--port", str(port),
                         "--save", "", "--appendonly", "no", *extra])
        log = self.stack.enter_context((directory / "redis.log").open("w+"))
        proc = subprocess.Popen(argv, cwd=directory, stdout=log, stderr=subprocess.STDOUT)

        def stop():
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()

        self.stack.callback(stop)
        deadline = time.monotonic() + 8
        while proc.poll() is None and time.monotonic() <= deadline:
            try:
                if self.cli(port, "PING", ca=ca) == "PONG":
                    return
            except subprocess.CalledProcessError:
                pass
            time.sleep(.1)
        log.seek(0)
        raise RuntimeError(log.read())

    def store(self, port, database="0", options=None):
        profile = {"id": "tools", "name": "Native Redis", "databaseType": "redis",
                   "database": database, "host": "127.0.0.1", "port": port, "readOnly": False,
                   "options": options}
        Path(self.env["DATAZEN_NATIVE_CONNECTIONS"]).write_text(
            json.dumps({"connections": [profile]}), encoding="utf-8"
        )

    def replay(self, name, actions, at):
        self.counter += 1
        script = self.root / f"script-{self.counter}.json"
        tree = self.root / f"tree-{self.counter}.json"
        script.write_text(json.dumps(actions), encoding="utf-8")
        result = subprocess.run(
            [str(self.args.binary), "--script", str(script), "--tree", str(tree), "--realtime",
             "--size", "1440x900", "--scale", "1", "--at", str(at)],
            cwd=self.root, env=self.env, text=True, capture_output=True, timeout=90,
        )
        assert result.returncode == 0, result.stdout + result.stderr
        raw = tree.read_text(encoding="utf-8")
        if self.args.artifacts:
            (self.args.artifacts / f"{name}.json").write_text(raw, encoding="utf-8")
        return json.loads(raw)


OPEN = [tap(.5, "conn-open-tools")]
TOOLS = [*OPEN, tap(3.2, "native-kv-tool-toggle")]


def output(tree):
    return element(tree, "native-kv-tool-output")["text"]
