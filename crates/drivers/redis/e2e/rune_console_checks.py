"""Rune Redis console and binary value journeys."""
import json
import subprocess

from rune_standalone import element, tap
from rune_tools_common import OPEN, TOOLS, enter, output


def console_completion(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "name with spaces", "tab-completed-value") == "OK"
    press = lambda time, key: {"t": time, "type": "key", "name": key}
    command = [*TOOLS, *enter(4.1, "native-kv-console-input", "PIN"),
               press(5.4, "Tab"), press(5.8, "Enter")]
    assert "PONG" in output(lab.replay("console-tab-command", command, 7))
    candidate = [*TOOLS, *enter(4.1, "native-kv-console-input", "GET name")]
    tree = lab.replay("console-key-candidate", candidate, 5.2)
    assert element(tree, "native-kv-complete-key-0")["label"] == "name with spaces"
    picked = [*candidate, press(5.6, "Tab"), press(6.1, "Enter")]
    assert "tab-completed-value" in output(lab.replay("console-tab-key", picked, 7.4))
    print("PASS: Tab accepts command and exact loaded key suggestions; Enter submits")



def binary_view(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    data = bytes.fromhex("81 a1 78 07")  # MessagePack {"x": 7}
    result = subprocess.check_output([lab.args.cli, "-h", "127.0.0.1", "-p", str(port),
                                      "-x", "--raw", "SET", "native-binary"], input=data, timeout=8)
    assert result.strip() == b"OK"
    selected = [*OPEN, tap(3, "native-kv-key-0")]
    preview = element(lab.replay("binary-raw", selected, 4.6), "native-kv-value")["text"]
    assert "Binary string (4 bytes)" in preview and "gaF4Bw==" in preview, preview
    tools = [*selected, tap(4.4, "native-kv-tool-toggle")]
    result = json.loads(output(lab.replay("binary-msgpack", [
        *tools, tap(5.5, "native-kv-decode-string"),
    ], 7)))
    assert result["ok"] and json.loads(result["json"]) == {"x": 7}, result
    invalid = json.loads(output(lab.replay("binary-invalid-base64", [
        *tools, tap(5.5, "native-kv-codec"), tap(6.5, "native-kv-decode-string"),
    ], 8)))
    assert not invalid["ok"], invalid
    assert subprocess.check_output([lab.args.cli, "-h", "127.0.0.1", "-p", str(port),
                                    "--raw", "GET", "native-binary"], timeout=8) == data + b"\n"
    keys = [*TOOLS, *enter(4.1, "native-kv-console-input", "PING"),
            {"t": 5.5, "type": "key", "name": "Enter"}]
    tree = lab.replay("console-enter", keys, 7)
    assert element(tree, "native-kv-console-prompt")["text"] == "db0>"
    assert "PONG" in output(tree)
    print("PASS: binary preview, parse-only MessagePack, rejection without mutation, and Enter submit")



def console_batch(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    assert lab.cli(port, "SET", "键 名", "unicode-value") == "OK"
    queue_ping = [*TOOLS, *enter(4.1, "native-kv-console-input", "PING"),
                  tap(5.4, "native-kv-console-queue")]
    valid = [*queue_ping, *enter(6.0, "native-kv-console-input", 'GET "键 名"'),
             tap(7.5, "native-kv-console-queue")]
    result = output(lab.replay("console-batch", [*valid, tap(8.4, "native-kv-console-run-batch")], 9.8))
    assert "PONG" in result and "unicode-value" in result, result
    invalid = [*queue_ping, *enter(6, "native-kv-console-input", "SET unapproved value"),
               tap(7.5, "native-kv-console-queue"), tap(8.4, "native-kv-console-run-batch")]
    refused = output(lab.replay("console-batch-refused", invalid, 9.8))
    assert "read-only commands" in refused and "PONG" not in refused, refused
    assert lab.cli(port, "EXISTS", "unapproved") == "0"
    print("PASS: bounded console batch, Unicode quoted key and pre-dispatch write refusal")



def console_set(lab):
    port = lab.port()
    lab.start(port)
    lab.store(port)
    staged = [*TOOLS, *enter(4.1, "native-kv-console-input", "SET console-created initial"),
              {"t": 5.7, "type": "key", "name": "Enter"}]
    tree = lab.replay("console-create-preview", staged, 7)
    assert "without overwriting" in element(tree, "native-kv-console-set-review")["text"]
    assert lab.cli(port, "EXISTS", "console-created") == "0"
    actions = [*staged, tap(6.8, "native-kv-console-confirm-set")]
    assert "Key created" in output(lab.replay("console-create", actions, 8.1))
    assert lab.cli(port, "GET", "console-created") == "initial"
    assert lab.cli(port, "SET", "console-created", "retained") == "OK"
    assert "already exists" in output(lab.replay("console-create-refused", actions, 8.1))
    assert lab.cli(port, "GET", "console-created") == "retained"
    print("PASS: terminal SET requires review, can create a key, and never overwrites")
