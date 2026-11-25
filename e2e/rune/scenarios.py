"""Fixtures and Rune replay steps shared by the native journeys."""
import json
import sqlite3


def fixture(store, database):
    with sqlite3.connect(database) as db:
        db.executescript("""
            CREATE TABLE customers (id INTEGER PRIMARY KEY, customer_name TEXT NOT NULL);
            INSERT INTO customers VALUES (1, 'fixture-ada'), (2, 'fixture-lin');
        """)
    store.write_text(json.dumps({"connections": [{
        "id": "e2e-sqlite", "name": "Rune SQLite E2E", "databaseType": "sqlite",
        "database": str(database), "connectionTimeout": 30, "maxPoolSize": 4,
    }]}), encoding="utf-8")


def tap(t, key):
    return {"t": t, "type": "tap", "key": key}


def type_text(t, text):
    return {"t": t, "type": "type", "text": text, "every": 0.027}


def query_steps(sql, connection_id="e2e-sqlite"):
    return [
        tap(0.5, f"conn-open-{connection_id}"),
        tap(3, "conn-toolbar-new-query"),
        tap(4, "sql-editor-scroll"),
        {"t": 4.5, "type": "key", "name": "a", "cmd": True},
        type_text(4.8, sql),
        tap(8.5, "sql-run"),
    ]


def visible(tree, *, key=None, text=None, within=None):
    elements = tree["elements"]
    viewport_w, viewport_h = tree["viewport"]
    parents = {node["path"] for node in elements if node.get("key") == within} if within else None
    for node in elements:
        x, y, width, height = node["rect"]
        if width <= 0 or height <= 0 or node["opacity"] <= 0 or node.get("exiting"):
            continue
        if x >= viewport_w or y >= viewport_h or x + width <= 0 or y + height <= 0:
            continue
        if key is not None and node.get("key") != key:
            continue
        if text is not None and node.get("text") != text:
            continue
        if parents is not None and not any(node["path"].startswith(p + "/") for p in parents):
            continue
        return True
    return False
