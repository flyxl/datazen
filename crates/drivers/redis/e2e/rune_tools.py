#!/usr/bin/env python3
"""Native Redis tool, topology, and DB routing integration checks."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import zipfile

from rune_standalone import element, tap
from rune_tools_common import Lab, OPEN, TOOLS, enter, output
from rune_console_checks import binary_view, console_batch, console_completion, console_set


def standalone(lab):
    port = lab.port()
    lab.start(port)
    assert lab.cli(port, "SET", "native-key", "database-zero") == "OK"
    assert lab.cli(port, "SET", "native-key", "database-one", db=1) == "OK"
    assert lab.cli(port, "RPUSH", "native-list", "old-list") == "1"
    assert lab.cli(port, "SADD", "native-set", "old-set") == "1"
    assert lab.cli(port, "ZADD", "native-zset", "4", "native-member") == "1"
    assert lab.cli(port, "XADD", "native-stream", "*", "origin", "original")
    lab.store(port)

    db_one = lab.replay("db-one", [*OPEN, tap(3, "native-kv-db-next"), tap(5, "native-kv-key-0")], 7)
    assert element(db_one, "native-kv-context")["text"] == "Redis · DB 1"
    assert element(db_one, "native-kv-value")["text"] == lab.cli(port, "GET", "native-key", db=1)
    basics = lab.replay("lookup-collection", OPEN, 3)
    buttons = {n.get("label"): n["key"] for n in basics["elements"]
               if n.get("key", "").startswith("native-kv-key-")}
    for key, value, check in (
        ("native-list", "new-list", ("LINDEX", "native-list", "0")),
        ("native-set", "new-set", ("SISMEMBER", "native-set", "new-set")),
        ("native-zset", "12", ("ZSCORE", "native-zset", "native-member")),
        ("native-stream", "field=saved", ("XLEN", "native-stream")),
    ):
        tree = lab.replay(key, [*OPEN, tap(3.3, buttons[key]), tap(4.4, "native-kv-edit-collection-0"),
                                *enter(5.2, "native-kv-edit-input", value), tap(6.6, "native-kv-save")], 8.2)
        assert element(tree, "native-kv-edit-notice")["text"] == "Saved to Redis."
        assert lab.cli(port, *check) == {
            "native-list": "new-list", "native-set": "1", "native-zset": "12", "native-stream": "2",
        }[key]

    console = [*TOOLS, *enter(4.1, "native-kv-console-input", "GET native-key"),
               tap(5.7, "native-kv-console-run")]
    response = lab.replay("console", console, 7)
    assert "database-zero" in output(response)
    recall = [*TOOLS, *enter(4.1, "native-kv-console-input", "GE"),
              tap(5.0, "native-kv-complete-GET"), tap(5.4, "native-kv-console-input"),
              {"t": 5.6, "type": "key", "name": "End"},
              {"t": 5.9, "type": "type", "text": "native-key", "every": .025},
              tap(6.7, "native-kv-console-run"),
              *enter(8, "native-kv-console-input", "PING"), tap(9.3, "native-kv-console-older"),
              tap(10.2, "native-kv-console-run")]
    assert "database-zero" in output(lab.replay("console-history", recall, 11.6))
    blocked = lab.replay("console-refuses-write", [*TOOLS, *enter(4.1, "native-kv-console-input",
                                                                 "SET native-key not-allowed"),
                                                  tap(5.7, "native-kv-console-run")], 7)
    assert "read-only commands" in output(blocked)
    assert lab.cli(port, "GET", "native-key") == "database-zero"
    overview = output(lab.replay("overview", [*TOOLS, tap(4.1, "native-kv-overview")], 5.7))
    assert "Redis version:" in overview and "DB 1: keys=1" in overview, overview
    assert "Recent slow commands" in overview
    memory = output(lab.replay("memory-sample", [*TOOLS, tap(4.1, "native-kv-memory-sample")], 5.7))
    assert "native-key" in memory and '"bytes"' in memory, memory
    info = lab.replay("info", [*TOOLS, tap(4.1, "native-kv-info")], 5.7)
    assert "redis_version" in output(info)
    lab.cli(port, "CONFIG", "SET", "slowlog-log-slower-than", "0")
    lab.cli(port, "GET", "native-key")
    slow = lab.replay("slowlog", [*TOOLS, tap(4.1, "native-kv-slowlog")], 5.7)
    assert "native-key" in output(slow)

    mon = lab.replay("monitor", [*TOOLS, tap(4.2, "native-kv-monitor-start"),
                                 *enter(6, "native-kv-console-input", "GET native-key"),
                                 tap(7.5, "native-kv-console-run"),
                                 tap(9.5, "native-kv-monitor-read")], 11)
    assert "native-key" in output(mon)
    print("PASS: real Redis DB switching, List/Set/ZSet/Stream edits, console policy, INFO, slow log and monitor")


def pubsub(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    subscribed = [*TOOLS, *enter(4.2, "native-kv-channel-patterns", "native-*"),
                  tap(5.8, "native-kv-subscribe"), *enter(6.9, "native-kv-channel", "native-events"),
                  *enter(8.25, "native-kv-message", "native-payload"),
                  tap(9.5, "native-kv-publish"), tap(11.2, "native-kv-read-messages")]
    result = output(lab.replay("pubsub", subscribed, 12.7))
    assert "native-events" in result and "native-payload" in result, result
    stopped = lab.replay("pubsub-stop", [*subscribed, tap(13.1, "native-kv-unsubscribe")], 14.4)
    assert "Unsubscribed" in output(stopped)
    print("PASS: real Redis pattern Pub/Sub and unsubscribe")


def search(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    lab.cli(port, "SET", "cache:item:first", "found-target-needle")
    lab.cli(port, "SET", "cache:item:second", "no match")
    lab.cli(port, "SET", "outside", "excluded-target-needle")
    expanded = [*OPEN, tap(3.2, "native-kv-tree-toggle")]
    tree = lab.replay("tree-expanded", expanded, 4.6)
    assert element(tree, "native-kv-folder-cache:item:")["label"]
    collapsed = lab.replay("tree-collapsed", [*expanded, tap(4.7, "native-kv-folder-cache:")], 6.3)
    assert not any(n.get("key") == "native-kv-folder-cache:item:" for n in collapsed["elements"])
    result = output(lab.replay("value-search", [
        *OPEN, *enter(3, "native-kv-pattern", "cache:*"), tap(4.5, "native-kv-pattern-apply"),
        tap(5.7, "native-kv-tool-toggle"), *enter(6.7, "native-kv-value-search", "target-needle"),
        tap(8.3, "native-kv-value-search-start"),
    ], 10))
    assert "cache:item:first" in result, result
    assert "cache:item:second" not in result and "outside" not in result, result
    print("PASS: native grouped key tree and Redis value search with a key filter")


def lifecycle(lab):
    port = lab.port()
    lab.start(port)
    lab.cli(port, "SET", "target-existing", "keep")
    lab.store(port)

    def select(name):
        return [*OPEN, *enter(3.2, "native-kv-pattern", name),
                tap(4.7, "native-kv-pattern-apply"), tap(6.2, "native-kv-key-0")]

    create = [*TOOLS, *enter(4.1, "native-kv-manage-key", "created"),
              *enter(5.5, "native-kv-manage-value", "written"), tap(7.0, "native-kv-create")]
    assert "Key created" in output(lab.replay("create", create, 8.5))
    assert lab.cli(port, "GET", "created") == "written"
    assert "already exists" in output(lab.replay("create-duplicate", create, 8.5))
    assert lab.cli(port, "GET", "created") == "written"

    manage = [*select("created"), tap(7.3, "native-kv-tool-toggle")]
    fail = [*manage, *enter(8.0, "native-kv-manage-key", "target-existing"),
            tap(9.7, "native-kv-rename")]
    assert "already exists" in output(lab.replay("rename-conflict", fail, 11))
    assert lab.cli(port, "GET", "target-existing") == "keep"
    rename = [*manage, *enter(8.0, "native-kv-manage-key", "renamed"), tap(9.7, "native-kv-rename")]
    assert "Key renamed" in output(lab.replay("rename", rename, 11))
    assert lab.cli(port, "GET", "renamed") == "written" and lab.cli(port, "EXISTS", "created") == "0"

    chosen = [*select("renamed"), tap(7.3, "native-kv-tool-toggle")]
    bundle = lab.root / "trusted.zip"
    export = [*chosen, tap(8.2, "native-kv-export"),
              {"t": 9.4, "type": "file-dialog", "path": str(bundle)}]
    assert "Exported 1 keys" in output(lab.replay("export", export, 11.5))
    with zipfile.ZipFile(bundle) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["schemaVersion"] == 1 and manifest["keys"][0]["key"] == "renamed"
        assert archive.read(manifest["keys"][0]["dumpFile"])

    bad = [*chosen, *enter(8.0, "native-kv-manage-key", "wrong"), tap(9.7, "native-kv-delete")]
    assert "exact selected key name" in output(lab.replay("delete-refused", bad, 11))
    assert lab.cli(port, "EXISTS", "renamed") == "1"
    delete = [*chosen, *enter(8.0, "native-kv-manage-key", "renamed"), tap(9.7, "native-kv-delete")]
    assert "Key deleted" in output(lab.replay("delete", delete, 11))
    assert lab.cli(port, "EXISTS", "renamed") == "0"

    restore = [*TOOLS, tap(4.2, "native-kv-import"),
               {"t": 5.5, "type": "file-dialog", "path": str(bundle)}]
    assert "Restored 1 of 1" in output(lab.replay("restore", restore, 7.7))
    assert lab.cli(port, "GET", "renamed") == "written"
    lab.cli(port, "SET", "renamed", "should-not-overwrite")
    assert "Restored 0 of 1" in output(lab.replay("restore-conflict", restore, 7.7))
    assert lab.cli(port, "GET", "renamed") == "should-not-overwrite"
    print("PASS: filter, no-overwrite create/rename, typed delete, and manifest-v1 ZIP round trip")


def topology(lab):
    master = lab.port()
    lab.start(master)
    lab.cli(master, "SET", "sentinel-key", "sentinel-value")
    sentinel = lab.port()
    lab.start(sentinel, sentinel=master)
    lab.store(sentinel, options={"topology": "sentinel", "sentinelNodes": [f"127.0.0.1:{sentinel}"],
                                 "sentinelMasterName": "native-test"})
    tree = lab.replay("sentinel", [*OPEN, tap(4, "native-kv-key-0")], 6.5)
    assert element(tree, "native-kv-value")["text"] == "sentinel-value"

    port = lab.port(cluster=True)
    lab.start(port, ["--cluster-enabled", "yes", "--cluster-config-file", "nodes.conf",
                     "--cluster-node-timeout", "1000"])
    assert lab.cli(port, "CLUSTER", "ADDSLOTSRANGE", "0", "16383") == "OK"
    deadline = time.monotonic() + 8
    while "cluster_state:ok" not in lab.cli(port, "CLUSTER", "INFO"):
        assert time.monotonic() < deadline
        time.sleep(.15)
    assert lab.cli(port, "SET", "cluster-key", "cluster-value") == "OK"
    lab.store(port, options={"topology": "cluster"})
    tree = lab.replay("cluster", [*OPEN, tap(4, "native-kv-key-0"), tap(5, "native-kv-db-next")], 6.8)
    assert element(tree, "native-kv-context")["text"] == "Redis · DB 0"
    assert element(tree, "native-kv-value")["text"] == "cluster-value"
    print("PASS: real Sentinel and single-node Cluster profiles")




def workbench_layout(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "db0-key", "zero") == "OK"
    assert lab.cli(port, "SET", "db2-key", "two", db=2) == "OK"
    tree = lab.replay("key-browser-while-tools", TOOLS, 4.5)
    assert element(tree, "native-kv-keys") and element(tree, "native-kv-console-prompt")
    actions = [*TOOLS, tap(4.5, "native-kv-db-picker"), tap(5.5, "native-kv-choose-db-2")]
    tree = lab.replay("direct-db-picker", actions, 7.2)
    assert element(tree, "native-kv-context")["text"] == "Redis · DB 2"
    assert element(tree, "native-kv-console-prompt")["text"] == "db2>"
    assert element(tree, "native-kv-key-0")["label"] == "db2-key"
    scoped = lab.replay("tools-console-tab", [*OPEN, tap(3.2, "native-kv-tab-console")], 5)
    controls = {n.get("key") for n in scoped["elements"]}
    assert {"native-kv-console-input", "native-kv-keys"} <= controls
    assert "native-kv-info" not in controls and "native-kv-subscribe" not in controls
    scoped = lab.replay("tools-pubsub-tab", [*OPEN, tap(3.2, "native-kv-tab-pubsub")], 5)
    controls = {n.get("key") for n in scoped["elements"]}
    assert {"native-kv-subscribe", "native-kv-keys"} <= controls
    assert "native-kv-console-input" not in controls
    overview = lab.replay("tools-overview-shortcut", [*OPEN, tap(3.2, "native-kv-quick-overview")], 5)
    assert "Redis version:" in output(overview)
    print("PASS: key browser remains visible with tools tabs, direct DB selection and quick overview")






def server_folders(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "demo?:branch:leaf", "server-folder-leaf") == "OK"
    assert lab.cli(port, "SET", "demox:branch:other", "different-namespace") == "OK"
    actions = [*OPEN, tap(3.6, "native-kv-server-folders")]
    tree = lab.replay("server-folder-root", actions, 5.1)
    assert element(tree, "native-kv-server-folder-0")["label"] == "demo?:"
    assert element(tree, "native-kv-server-folder-1")["label"] == "demox:"
    nested = [*actions, tap(5.3, "native-kv-server-folder-0")]
    tree = lab.replay("server-folder-level-one", nested, 6.7)
    assert element(tree, "native-kv-server-folder-0")["label"] == "demo?:branch:"
    assert not any(n.get("label") == "demox:branch:" for n in tree["elements"])
    leaf = [*nested, tap(7, "native-kv-server-folder-0"), tap(8.6, "native-kv-server-leaf-0")]
    tree = lab.replay("server-folder-leaf", leaf, 10)
    assert element(tree, "native-kv-value")["text"] == "server-folder-leaf"
    back = [*leaf, tap(10.3, "native-kv-server-folder-up")]
    tree = lab.replay("server-folder-up", back, 11.8)
    assert element(tree, "native-kv-current-folder")["text"] == "Folder: demo?:"
    print("PASS: server-paged nested literal namespaces, detail and Up navigation")


def key_filters(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "permanent", "string") == "OK"
    assert lab.cli(port, "SET", "expires", "string", "EX", "120") == "OK"
    assert lab.cli(port, "HSET", "other-type", "field", "value") == "1"

    def names(tree):
        return {n.get("label") for n in tree["elements"]
                if n.get("key", "").startswith("native-kv-key-")}

    type_filter = [*OPEN, tap(3.2, "native-kv-type-filter")]
    assert names(lab.replay("key-filter-type", type_filter, 4.7)) == {"permanent", "expires"}
    both = [*type_filter, tap(4.8, "native-kv-persistent-filter")]
    assert names(lab.replay("key-filter-persistent", both, 6.2)) == {"permanent"}
    hash_only = [*both, tap(6.4, "native-kv-type-filter")]
    assert names(lab.replay("key-filter-hash", hash_only, 7.8)) == {"other-type"}
    print("PASS: server-side type and no-expiry key filters combine correctly")


def cluster_multishard(lab):
    import binascii

    ports = [lab.port(cluster=True), None]
    lab.start(ports[0], ["--cluster-enabled", "yes", "--cluster-config-file", "nodes.conf",
                         "--cluster-node-timeout", "1000"])
    ports[1] = lab.port(cluster=True)
    lab.start(ports[1], ["--cluster-enabled", "yes", "--cluster-config-file", "nodes.conf",
                         "--cluster-node-timeout", "1000"])
    assert lab.cli(ports[0], "CLUSTER", "ADDSLOTSRANGE", "0", "8191") == "OK"
    assert lab.cli(ports[1], "CLUSTER", "ADDSLOTSRANGE", "8192", "16383") == "OK"
    assert lab.cli(ports[0], "CLUSTER", "MEET", "127.0.0.1", str(ports[1])) == "OK"
    deadline = time.monotonic() + 15
    while not all("cluster_state:ok" in lab.cli(port, "CLUSTER", "INFO") for port in ports):
        assert time.monotonic() < deadline, "Cluster did not converge"
        time.sleep(.15)

    def shard(key):
        return (binascii.crc_hqx(key.encode("ascii"), 0) % 16384) // 8192

    anchor = shard("datazen:redis:workbench:sample-anchor")
    near, remote = [], None
    for i in range(500):
        key = f"multishard-{i:03}"
        if shard(key) == anchor and len(near) < 90:
            assert lab.cli(ports[anchor], "SET", key, "on-browse-shard") == "OK"
            near.append(key)
        if shard(key) != anchor and remote is None:
            assert lab.cli(ports[1 - anchor], "SET", key, "on-other-shard") == "OK"
            remote = key
        if len(near) == 90 and remote is not None:
            break
    assert len(near) == 90 and remote is not None
    remote_port = ports[1 - anchor]
    assert int(lab.cli(remote_port, "CLUSTER", "KEYSLOT", remote)) // 8192 != anchor
    assert lab.cli(remote_port, "GET", remote) == "on-other-shard"

    # Connect through the non-anchor seed. Browse cursors must remain on the
    # legacy browse shard; exact lookup must still route to the other primary.
    lab.store(remote_port, options={"topology": "cluster"})
    first = lab.replay("cluster-two-primaries-first", OPEN, 4)
    keys = lambda tree: {n.get("label") for n in tree["elements"]
                         if n.get("key", "").startswith("native-kv-key-")}
    initial = keys(first)
    assert initial and initial <= set(near), initial
    later = lab.replay("cluster-two-primaries-next", [*OPEN, tap(4, "native-kv-more")], 5.8)
    expanded = keys(later)
    assert len(expanded) > len(initial) and expanded <= set(near)
    remote_key = lab.replay("cluster-two-primaries-exact", [
        *OPEN, *enter(3.3, "native-kv-pattern", remote), tap(5, "native-kv-pattern-apply"),
        tap(6.3, "native-kv-key-0"),
    ], 7.7)
    assert element(remote_key, "native-kv-value")["text"] == "on-other-shard"
    literal = next(f"literal?[{i}]*" for i in range(100)
                   if shard(f"literal?[{i}]*") != anchor)
    assert lab.cli(remote_port, "SET", literal, "literal-name") == "OK"
    opened = lab.replay("cluster-two-primaries-literal", [
        *OPEN, *enter(3.3, "native-kv-pattern", literal), tap(5, "native-kv-exact"),
    ], 6.7)
    assert element(opened, "native-kv-value")["text"] == "literal-name"
    topology_result = json.loads(output(lab.replay("cluster-topology-tool", [
        *TOOLS, tap(4.3, "native-kv-cluster-nodes"),
    ], 6.2)))
    primary_addresses = {node["addr"] for node in topology_result["nodes"] if node["role"] == "master"}
    assert primary_addresses == {f"127.0.0.1:{port}" for port in ports}, topology_result
    print("PASS: two Cluster primaries, anchored pagination, slot-routed exact lookup and discovered primaries")


def batch_delete(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    names = ("red-delete-a", "red-delete-b", "red-keep")
    for key in names:
        assert lab.cli(port, "SET", key, "original") == "OK"
    basics = lab.replay("lookup-marked-delete", OPEN, 3)
    buttons = {n.get("label"): n["key"] for n in basics["elements"]
               if n.get("key", "").startswith("native-kv-key-")}
    marked = [*OPEN, tap(3, buttons[names[0]]), tap(4, "native-kv-tool-toggle"),
              tap(5, "native-kv-mark-export"), tap(6, "native-kv-tool-toggle"),
              tap(7, buttons[names[1]]), tap(8, "native-kv-tool-toggle"),
              tap(9, "native-kv-mark-export"), tap(10, "native-kv-review-marked")]
    preview = element(lab.replay("marked-delete-preview", marked, 11), "native-kv-delete-preview")["text"]
    assert names[0] in preview and names[1] in preview and names[2] not in preview
    refused = [*marked, *enter(11.1, "native-kv-delete-confirm", "DELETE 3"),
               tap(12.8, "native-kv-delete-marked")]
    assert "exact number" in output(lab.replay("marked-delete-refused", refused, 14))
    assert lab.cli(port, "EXISTS", *names) == "3"
    confirm = [*marked, *enter(11.1, "native-kv-delete-confirm", "DELETE 2"),
               tap(12.8, "native-kv-delete-marked")]
    summary = output(lab.replay("marked-delete-confirmed", confirm, 14))
    assert "Deleted 2 of 2" in summary, summary
    assert lab.cli(port, "EXISTS", names[0], names[1]) == "0"
    assert lab.cli(port, "GET", names[2]) == "original"
    print("PASS: exact marked batch deletion requires review and preserves unmarked keys")


def connection_probe(lab):
    port = lab.port()
    lab.start(port)
    def probe(name, address):
        actions = [tap(.5, "connections-create-first"),
                   *enter(1.3, "connection-engine-input", "redis"),
                   *enter(2.6, "connection-address-input", address),
                   tap(4.4, "connection-test")]
        return element(lab.replay(name, actions, 8),
                       "native-redis-probe-status")["text"]
    assert "connection succeeded" in probe("probe-success", f"127.0.0.1:{port}")
    assert not Path(lab.env["DATAZEN_NATIVE_CONNECTIONS"]).exists()
    assert "failed" in probe("probe-unreachable", f"127.0.0.1:{lab.port()}")
    assert not Path(lab.env["DATAZEN_NATIVE_CONNECTIONS"]).exists()
    print("PASS: native Redis form probes live and unreachable endpoints without saving")


def connection_form(lab):
    host, other = lab.port(), lab.port()
    lab.start(host)
    addresses = f"127.0.0.1:{host},127.0.0.1:{other}"
    actions = [tap(.5, "connections-create-first"),
               *enter(1.3, "connection-name-input", "Sentinel fixture"),
               *enter(2.6, "connection-engine-input", "redis"),
               *enter(3.55, "connection-address-input", f"127.0.0.1:{host}"),
               tap(5, "native-redis-topology"), tap(5.5, "native-redis-read-only"),
               *enter(6.0, "native-redis-sentinel-master", "native-test"),
               *enter(7.5, "native-redis-nodes", addresses),
               tap(9.4, "connection-save")]
    tree = lab.replay("connection-form", actions, 11.1)
    assert not any(n.get("key") == "connection-dialog" for n in tree["elements"])
    stored = json.loads(Path(lab.env["DATAZEN_NATIVE_CONNECTIONS"]).read_text(encoding="utf-8"))
    profile = next(p for p in stored["connections"] if p["name"] == "Sentinel fixture")
    assert profile["databaseType"] == "redis" and profile["readOnly"] is True
    opt = profile["options"]
    assert opt["topology"] == "sentinel" and opt["sentinelMasterName"] == "native-test"
    assert opt["sentinelNodes"] == addresses.split(",")
    print("PASS: native Redis connection form persists topology, multiple nodes, and read-only")


def removals(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "HSET", "hash-key", "discard", "old", "retain", "original") == "2"
    assert lab.cli(port, "RPUSH", "list-key", "discard", "retain") == "2"
    assert lab.cli(port, "SADD", "set-key", "discard", "retain") == "2"
    assert lab.cli(port, "ZADD", "zset-key", "2", "discard", "3", "retain") == "2"
    basics = lab.replay("lookup-removals", OPEN, 3)
    buttons = {n.get("label"): n["key"] for n in basics["elements"]
               if n.get("key", "").startswith("native-kv-key-")}

    def remove(key, label, confirmation="REMOVE"):
        selected = [*OPEN, tap(3, buttons[key])]
        editor = lab.replay("remove-controls-" + key, selected, 4.4)
        edit_key = next(n["key"] for n in editor["elements"] if n.get("label") == label)
        return element(lab.replay("remove-save-" + key, [*selected, tap(4.6, edit_key),
            *enter(5.2, "native-kv-edit-input", confirmation), tap(6.8, "native-kv-save")], 8),
            "native-kv-edit-notice")["text"]

    assert "confirm" in remove("hash-key", "Remove Hash field discard (type REMOVE)", "WRONG")
    assert lab.cli(port, "HGET", "hash-key", "discard") == "old"
    for key, label, command, expected in (
        ("hash-key", "Remove Hash field discard (type REMOVE)", ("HKEYS", "hash-key"), "retain"),
        ("list-key", "Remove list head discard (type REMOVE)", ("LRANGE", "list-key", "0", "-1"), "retain"),
        ("set-key", "Remove Set member discard (type REMOVE)", ("SMEMBERS", "set-key"), "retain"),
        ("zset-key", "Remove ZSet member discard (type REMOVE)", ("ZRANGE", "zset-key", "0", "-1"), "retain"),
    ):
        assert remove(key, label) == "Saved to Redis."
        assert lab.cli(port, *command) == expected
    assert "last one" in remove("list-key", "Remove list head retain (type REMOVE)")
    assert lab.cli(port, "LRANGE", "list-key", "0", "-1") == "retain"
    print("PASS: guarded removal of Hash fields, List head, Set/ZSet members, and final-entry protection")


def additions(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "HSET", "hash-key", "keep", "original") == "1"
    assert lab.cli(port, "RPUSH", "list-key", "first") == "1"
    assert lab.cli(port, "ZADD", "zset-key", "2", "keep") == "1"
    basics = lab.replay("lookup-additions", OPEN, 3)
    buttons = {n.get("label"): n["key"] for n in basics["elements"]
               if n.get("key", "").startswith("native-kv-key-")}

    def add(key, label, value):
        selected = [*OPEN, tap(3, buttons[key])]
        editor = lab.replay("controls-" + key, selected, 4.5)
        edit_key = next(n["key"] for n in editor["elements"] if n.get("label") == label)
        return element(lab.replay("save-" + key, [*selected, tap(4.5, edit_key),
            *enter(5.2, "native-kv-edit-input", value), tap(6.8, "native-kv-save")], 8.4),
            "native-kv-edit-notice")["text"]

    assert add("hash-key", "Add field as field=value", "other=added") == "Saved to Redis."
    assert lab.cli(port, "HGET", "hash-key", "other") == "added"
    assert "changed" in add("hash-key", "Add field as field=value", "keep=overwritten")
    assert lab.cli(port, "HGET", "hash-key", "keep") == "original"
    assert add("list-key", "Append list item", "second") == "Saved to Redis."
    assert lab.cli(port, "LINDEX", "list-key", "1") == "second"
    assert add("zset-key", "Add member as member=score", "other=4.25") == "Saved to Redis."
    assert lab.cli(port, "ZSCORE", "zset-key", "other") == "4.25"
    assert "changed" in add("zset-key", "Add member as member=score", "keep=12")
    assert lab.cli(port, "ZSCORE", "zset-key", "keep") == "2"
    print("PASS: guarded Hash field addition, List append and Sorted Set member addition")


def stream_groups(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    existing = lab.cli(port, "XADD", "stream-key", "*", "message", "hello")
    assert lab.cli(port, "XGROUP", "CREATE", "stream-key", "workers", "0") == "OK"
    assert existing in lab.cli(port, "XREADGROUP", "GROUP", "workers", "reader", "COUNT", "1",
                               "STREAMS", "stream-key", ">")
    opened = [*OPEN, tap(3, "native-kv-key-0"), tap(4, "native-kv-tool-toggle")]
    pending = [*opened, *enter(5.1, "native-kv-stream-group", "workers")]
    result = output(lab.replay("stream-pending", [*pending, tap(6.7, "native-kv-stream-pending")], 8))
    assert existing in result and "reader" in result, result
    ack = [*pending, *enter(6.75, "native-kv-stream-entry", existing),
           tap(8.25, "native-kv-stream-ack")]
    assert "Acknowledged one" in output(lab.replay("stream-ack", ack, 9.7))
    assert lab.cli(port, "XPENDING", "stream-key", "workers", "-", "+", "10") == ""
    create = [*opened, *enter(5.1, "native-kv-stream-group", "new-group"),
              tap(6.8, "native-kv-stream-create")]
    result = output(lab.replay("stream-create-group", create, 8))
    assert "Consumer group created" in result, result
    assert "new-group" in lab.cli(port, "XINFO", "GROUPS", "stream-key")
    assert "group exists" in output(lab.replay("stream-group-conflict", create, 8))
    print("PASS: Stream pending entries, exact acknowledgement, and existing-only group creation")


def inspection(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "plain-key", "simple") == "OK"
    msg = lab.cli(port, "XADD", "stream-key", "*", "message", "hello")
    assert lab.cli(port, "XGROUP", "CREATE", "stream-key", "workers", "0") == "OK"
    assert msg in lab.cli(port, "XREADGROUP", "GROUP", "workers", "reader", "COUNT", "1",
                          "STREAMS", "stream-key", ">")
    basics = lab.replay("lookup-inspection", OPEN, 3)
    buttons = {n.get("label"): n["key"] for n in basics["elements"]
               if n.get("key", "").startswith("native-kv-key-")}
    stream = [*OPEN, tap(3, buttons["stream-key"]), tap(4.0, "native-kv-tool-toggle")]
    assert "workers" in output(lab.replay("stream-groups", [*stream, tap(5, "native-kv-stream-groups")], 6.5))
    consumers = [*stream, *enter(5.1, "native-kv-stream-group", "workers"),
                 tap(6.6, "native-kv-stream-consumers")]
    assert "reader" in output(lab.replay("stream-consumers", consumers, 8.0))
    plain = [*OPEN, tap(3, buttons["plain-key"]), tap(4.0, "native-kv-tool-toggle")]
    props = output(lab.replay("key-props", [*plain, tap(5, "native-kv-key-properties")], 6.5))
    assert "encoding" in props, props
    bundle = lab.root / "batch.zip"
    batch = [*plain, tap(5, "native-kv-mark-export"), tap(6, "native-kv-tool-toggle"),
             tap(7, buttons["stream-key"]), tap(8, "native-kv-tool-toggle"),
             tap(9, "native-kv-mark-export"), tap(10, "native-kv-export"),
             {"t": 11.2, "type": "file-dialog", "path": str(bundle)}]
    assert "Exported 2 keys" in output(lab.replay("batch-export", batch, 13))
    with zipfile.ZipFile(bundle) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        assert {row["key"] for row in manifest["keys"]} == {"plain-key", "stream-key"}
    print("PASS: key properties, Stream groups and consumers, and an explicitly marked batch ZIP")


def tls(lab):
    openssl = shutil.which("openssl")
    if not openssl:
        raise RuntimeError("The verified TLS journey requires openssl.")
    ca, ca_key = lab.root / "ca.crt", lab.root / "ca.key"
    cert, key, csr = (lab.root / name for name in ("server.crt", "server.key", "server.csr"))
    ext = lab.root / "extensions.cnf"
    ext.write_text("subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n", encoding="utf-8")

    def make(*args):
        subprocess.run([openssl, *args], check=True, capture_output=True, timeout=12)

    make("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-subj", "/CN=DataZen test CA", "-keyout", ca_key, "-out", ca)
    make("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=127.0.0.1",
         "-keyout", key, "-out", csr)
    make("x509", "-req", "-in", csr, "-CA", ca, "-CAkey", ca_key, "-CAcreateserial",
         "-days", "1", "-extfile", ext, "-out", cert)
    port = lab.port()
    lab.start(port, ["--port", "0", "--tls-port", str(port), "--tls-auth-clients", "no",
                     "--tls-cert-file", str(cert), "--tls-key-file", str(key),
                     "--tls-ca-cert-file", str(ca)], ca=ca)
    assert lab.cli(port, "SET", "tls-key", "verified-tls", ca=ca) == "OK"
    lab.store(port, options={"tls": {"enabled": True, "caPath": str(ca)}})
    tree = lab.replay("tls", [*OPEN, tap(4.5, "native-kv-key-0")], 6)
    assert element(tree, "native-kv-value")["text"] == "verified-tls"
    lab.store(port, options={"tls": {"enabled": True}})
    untrusted = lab.replay("tls-untrusted", OPEN, 6)
    assert not any(n.get("key") == "native-kv-key-0" for n in untrusted["elements"])
    print("PASS: native TLS with a trusted CA; an untrusted CA is rejected")


def redis_json(lab):
    port = lab.port()
    module = lab.args.redis_json_module
    lab.start(port, ["--loadmodule", str(module)] if module else [])
    lab.store(port)
    seeded = lab.cli(port, "JSON.SET", "profile", "$", '{"name":"before"}')
    assert seeded == "OK", "Test requires RedisJSON commands: " + seeded
    assert lab.cli(port, "EXPIRE", "profile", "120") == "1"
    tree = lab.replay("redis-json", [*OPEN, tap(3.3, "native-kv-key-0"),
        tap(4.4, "native-kv-edit-collection-0"),
        *enter(5.1, "native-kv-edit-input", '{"name":"after"}'),
        tap(6.8, "native-kv-save")], 8.3)
    assert element(tree, "native-kv-edit-notice")["text"] == "Saved to Redis."
    assert json.loads(lab.cli(port, "JSON.GET", "profile")) == {"name": "after"}
    assert 0 < int(lab.cli(port, "TTL", "profile")) <= 120
    print("PASS: RedisJSON native root edit preserves expiry")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--server", default=shutil.which("redis-server"))
    parser.add_argument("--cli", default=shutil.which("redis-cli"))
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--only", choices=("tools", "topology", "lifecycle", "pubsub", "search", "tls", "inspection", "redis-json", "additions", "form", "stream-groups", "batch-delete", "cluster-multishard", "key-filters", "console-batch", "removals", "probe", "binary-view", "layout", "console-completion", "console-set", "server-folders"))
    parser.add_argument("--redis-json-module", type=Path)
    args = parser.parse_args()
    if args.redis_json_module:
        args.redis_json_module = args.redis_json_module.resolve(strict=True)
    if not args.server or not args.cli:
        parser.error("Redis server and CLI executables are required.")
    args.binary = args.binary.resolve(strict=True)
    args.server, args.cli = os.path.abspath(args.server), os.path.abspath(args.cli)
    if args.artifacts:
        args.artifacts.mkdir(parents=True, exist_ok=True)
    with contextlib.ExitStack() as stack:
        lab = Lab(args, stack, Path(stack.enter_context(tempfile.TemporaryDirectory(prefix="rune-redis-tools-"))))
        if not args.only or args.only == "tools":
            standalone(lab)
        if not args.only or args.only == "pubsub":
            pubsub(lab)
        if not args.only or args.only == "search":
            search(lab)
        if args.only == "probe":
            connection_probe(lab)
        if args.only == "form":
            connection_form(lab)
        if not args.only or args.only == "removals":
            removals(lab)
        if not args.only or args.only == "additions":
            additions(lab)
        if not args.only or args.only == "batch-delete":
            batch_delete(lab)
        if not args.only or args.only == "stream-groups":
            stream_groups(lab)
        if not args.only or args.only == "inspection":
            inspection(lab)
        if not args.only or args.only == "topology":
            topology(lab)
        if not args.only or args.only == "console-set":
            console_set(lab)
        if not args.only or args.only == "console-completion":
            console_completion(lab)
        if not args.only or args.only == "layout":
            workbench_layout(lab)
        if not args.only or args.only == "binary-view":
            binary_view(lab)
        if not args.only or args.only == "console-batch":
            console_batch(lab)
        if not args.only or args.only == "server-folders":
            server_folders(lab)
        if not args.only or args.only == "key-filters":
            key_filters(lab)
        if not args.only or args.only == "cluster-multishard":
            cluster_multishard(lab)
        if args.only == "tls":
            tls(lab)
        if args.only == "redis-json" or (not args.only and args.redis_json_module):
            redis_json(lab)
        if not args.only or args.only == "lifecycle":
            lifecycle(lab)


if __name__ == "__main__":
    main()
