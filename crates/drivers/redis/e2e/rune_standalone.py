#!/usr/bin/env python3
"""Standalone DB 0 read/write E2E against a private loopback Redis."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import socket
import threading
import subprocess
import tempfile
import time


def tap(t, key):
    return {"t": t, "type": "tap", "key": key}


def fill(t, value):
    return [tap(t, "native-kv-edit-input"),
            {"t": t + .25, "type": "key", "name": "a", "cmd": True},
            {"t": t + .45, "type": "type", "text": value, "every": .03}]


def element(tree, key):
    matches = [node for node in tree["elements"]
               if node.get("key") == key and not node.get("exiting") and node["opacity"] > 0]
    assert len(matches) == 1, (key, matches)
    return matches[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--redis-server", default=shutil.which("redis-server"))
    parser.add_argument("--redis-cli", default=shutil.which("redis-cli"))
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    if not args.redis_server or not args.redis_cli:
        parser.error("Install Redis or pass both --redis-server and --redis-cli.")
    binary = args.binary.resolve(strict=True)
    # Preserve argv[0] on distributions where redis-server is a multi-call symlink.
    server_bin = Path(os.path.abspath(args.redis_server))
    cli_bin = Path(args.redis_cli).resolve(strict=True)
    if args.artifacts:
        args.artifacts.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="datazen-rune-redis-e2e-") as directory:
        root = Path(directory)
        with socket.socket() as free:
            free.bind(("127.0.0.1", 0))
            port = free.getsockname()[1]

        def cli(*command):
            return subprocess.check_output(
                [str(cli_bin), "-h", "127.0.0.1", "-p", str(port), "--raw", *command],
                text=True, stderr=subprocess.STDOUT, timeout=5,
            ).strip()

        with (root / "redis.log").open("w+", encoding="utf-8") as log:
            proc = subprocess.Popen(
                [str(server_bin), "--bind", "127.0.0.1", "--protected-mode", "yes",
                 "--port", str(port), "--dir", directory, "--save", "", "--appendonly", "no"],
                cwd=root, stdout=log, stderr=subprocess.STDOUT,
            )
            try:
                deadline = time.monotonic() + 8
                while True:
                    if proc.poll() is not None or time.monotonic() > deadline:
                        log.seek(0)
                        raise AssertionError(f"Private Redis did not start: {log.read()}")
                    try:
                        if cli("PING") == "PONG":
                            break
                    except subprocess.CalledProcessError:
                        pass
                    time.sleep(.1)
                for i in range(120):
                    if i % 2 == 0:
                        assert cli("SET", f"rune:string:{i}", f"value-{i}", "EX", "900") == "OK"
                    else:
                        assert cli("HSET", f"rune:hash:{i}", "sample", f"field-{i}") == "1"
                assert cli("DBSIZE") == "120"

                store = root / "connections.json"
                store.write_text(json.dumps({"connections": [{
                    "id": "redis-e2e", "name": "Redis E2E", "databaseType": "redis",
                    "database": "0", "host": "127.0.0.1", "port": port, "readOnly": True,
                }]}), encoding="utf-8")
                env = {**os.environ, "DATAZEN_NATIVE_CONNECTIONS": str(store),
                       "DATAZEN_NATIVE_KEYRING": "file", "XDG_DATA_HOME": str(root / "data"),
                       "XDG_CONFIG_HOME": str(root / "config"), "XDG_CACHE_HOME": str(root / "cache")}

                def replay(name, actions, at, on_mark=None):
                    script = root / "input.json"
                    tree = root / "tree.json"
                    tree.unlink(missing_ok=True)
                    script.write_text(json.dumps(actions), encoding="utf-8")
                    argv = [str(binary), "--script", str(script), "--tree", str(tree), "--realtime",
                            "--size", "1440x900", "--scale", "1", "--at", str(at)]
                    if on_mark is None:
                        result = subprocess.run(argv, cwd=root, env=env, text=True,
                                                capture_output=True, timeout=90)
                        assert result.returncode == 0, result.stdout + result.stderr
                    else:
                        notes, errors = [], []
                        with subprocess.Popen(argv, cwd=root, env=env, text=True,
                                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as app:
                            def watch():
                                for line in app.stdout:
                                    notes.append(line)
                                    if line.startswith("rune-mark external-write "):
                                        try:
                                            on_mark()
                                        except Exception as error:
                                            errors.append(error)
                            worker = threading.Thread(target=watch, daemon=True)
                            worker.start()
                            try:
                                code = app.wait(timeout=90)
                            finally:
                                if app.poll() is None:
                                    app.kill()
                                    app.wait(timeout=10)
                                worker.join(timeout=10)
                        assert code == 0, "".join(notes)
                        assert any(line.startswith("rune-mark external-write ") for line in notes), notes
                        assert not errors, errors
                    raw = tree.read_text(encoding="utf-8")
                    if args.artifacts:
                        (args.artifacts / f"{name}.tree.json").write_text(raw, encoding="utf-8")
                    return json.loads(raw)

                launch = [tap(.5, "conn-open-redis-e2e")]
                first = replay("first", launch, 3)
                assert element(first, "native-kv-context")["text"] == "Redis · DB 0 · Read only"
                assert not any(n.get("key") == "conn-toolbar-new-query" for n in first["elements"])
                first_count = int(element(first, "native-kv-loaded")["text"].split()[0])
                assert 0 < first_count < 120, first_count
                element(first, "native-kv-more")
                after = replay("more", [*launch, tap(3.4, "native-kv-more")], 5)
                after_count = int(element(after, "native-kv-loaded")["text"].split()[0])
                assert first_count < after_count <= 120, after_count
                loaded = [n["label"] for n in after["elements"]
                          if re.fullmatch(r"native-kv-key-\d+", n.get("key", ""))]
                assert len(loaded) == len(set(loaded)) == after_count

                choices = {}
                for node in first["elements"]:
                    label = node.get("label", "")
                    if re.fullmatch(r"native-kv-key-\d+", node.get("key", "")):
                        if int(node["key"].rsplit("-", 1)[1]) < 14:
                            choices.setdefault(label.split(":")[1], (node["key"], label))
                string_button, string_key = choices["string"]
                hash_button, hash_key = choices["hash"]
                ttl_before = int(cli("TTL", string_key))
                string = replay("string", [*launch, tap(3.4, string_button)], 5.2)
                ttl_after = int(cli("TTL", string_key))
                assert element(string, "native-kv-selected")["text"] == string_key
                assert element(string, "native-kv-type")["text"] == cli("TYPE", string_key)
                assert element(string, "native-kv-value")["text"] == cli("GET", string_key)
                assert not any(n.get("key") == "native-kv-edit-string" for n in string["elements"])
                ttl = int(element(string, "native-kv-ttl")["text"].removeprefix("TTL: ").removesuffix("s"))
                # Bound the app's snapshot between two observations of the same
                # Redis clock. Replaying and rendering can take longer on CI.
                assert 0 < ttl_after <= ttl <= ttl_before <= 900, (ttl_before, ttl, ttl_after)

                selected = replay("hash", [*launch, tap(3.4, string_button), tap(3.55, hash_button)], 5.2)
                assert element(selected, "native-kv-selected")["text"] == hash_key
                assert element(selected, "native-kv-type")["text"] == cli("TYPE", hash_key)
                assert element(selected, "native-kv-ttl")["text"] == "No expiry" and cli("TTL", hash_key) == "-1"
                assert element(selected, "native-kv-value")["text"] == f'"sample": "{cli("HGET", hash_key, "sample")}"'

                closed = replay("disconnect", [*launch, tap(3.4, "native-kv-disconnect")], 5)
                element(closed, "conn-open-redis-e2e")
                assert not any(n.get("key") == "native-redis-workbench" for n in closed["elements"])
                assert cli("DBSIZE") == "120"
                print("PASS: real Redis scan pagination, String/Hash/TTL, rapid selection, and disconnect")

                writable = json.loads(store.read_text(encoding="utf-8"))
                writable["connections"][0]["readOnly"] = False
                store.write_text(json.dumps(writable), encoding="utf-8")
                string_open = [*launch, tap(3.4, string_button)]
                draft = [*string_open, tap(4.5, "native-kv-edit-string"), *fill(5.2, "native-updated")]
                guarded = replay("dirty", [*draft, tap(6.5, hash_button), tap(7.2, "rail-settings"),
                                           tap(7.9, "native-kv-disconnect")], 9.2)
                assert element(guarded, "native-kv-selected")["text"] == string_key
                assert "Unsaved changes" in element(guarded, "native-kv-edit-notice")["text"]
                assert cli("GET", string_key) != "native-updated"
                dropped = replay("discard", [*draft, tap(6.5, "native-kv-discard"), tap(7.2, hash_button)], 9)
                assert element(dropped, "native-kv-selected")["text"] == hash_key
                assert cli("GET", string_key) != "native-updated"

                changed = replay("write-string", [*draft, tap(6.5, "native-kv-save")], 9)
                assert element(changed, "native-kv-value")["text"] == cli("GET", string_key) == "native-updated"
                assert int(cli("TTL", string_key)) > 0, "saving a value must preserve expiry"

                hash_open = [*launch, tap(3.4, hash_button)]
                changed_hash = replay("write-hash", [*hash_open, tap(4.5, "native-kv-edit-hash-0"),
                                                    *fill(5.2, "native-hash"), tap(6.5, "native-kv-save")], 9)
                assert cli("HGET", hash_key, "sample") == "native-hash"
                assert element(changed_hash, "native-kv-value")["text"] == '"sample": "native-hash"'

                expiring = replay("set-ttl", [*hash_open, tap(4.5, "native-kv-edit-ttl"),
                                               *fill(5.2, "90"), tap(6.5, "native-kv-save")], 9)
                assert element(expiring, "native-kv-ttl")["text"].startswith("TTL: ")
                assert 0 < int(cli("TTL", hash_key)) <= 90
                persistent = replay("clear-ttl", [*hash_open, tap(4.5, "native-kv-edit-ttl"),
                                                  *fill(5.2, "-1"), tap(6.5, "native-kv-save")], 9)
                assert element(persistent, "native-kv-ttl")["text"] == "No expiry"
                assert cli("TTL", hash_key) == "-1"

                # Race an independent client after the draft has been loaded,
                # but before the app's guarded save arrives.
                stale = replay("concurrent-write", [
                    *string_open, tap(4.5, "native-kv-edit-string"), *fill(5.2, "stale-draft"),
                    {"t": 6.4, "type": "mark", "name": "external-write"},
                    tap(8.0, "native-kv-save"),
                ], 10, on_mark=lambda: cli("SET", string_key, "external-value", "KEEPTTL"))
                assert "changed on Redis" in element(stale, "native-kv-edit-notice")["text"]
                assert cli("GET", string_key) == "external-value"
                assert cli("DBSIZE") == "120"
                print("PASS: String/Hash saves, TTL preservation/change/removal, unsaved guard, stale write rejection")
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


if __name__ == "__main__":
    main()
