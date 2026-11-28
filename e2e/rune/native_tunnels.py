"""Real stock Settings -> saved definitions. No tunnel or remote service runs."""
import base64
import json
import os
import stat

from scenarios import tap, visible


class SavedTunnelJourneys:
    def test_saved_http_proxy_settings_survive_restart_without_exposing_the_password(self):
        settings = self.root / "settings.json"
        settings.write_text('{"ui":{"language":"en","themeMode":"light"}}', encoding="utf-8")
        settings.chmod(0o600)
        saved_settings = settings.read_bytes()
        path = self.root / "tunnels.json"
        secret = "native-tunnel-test-fixture-927"
        entry = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-tunnels")]

        def put(t, key, value):
            return [
                tap(t, key),
                {"t": t + .12, "type": "key", "name": "a", "cmd": True},
                {"t": t + .25, "type": "paste", "text": value},
            ]

        prefix = [
            *entry, tap(2.3, "tunnel-create"),
            *put(3.3, "tunnel-edit-name", "Local proxy fixture"),
            *put(4.1, "new-conn-http-proxy-host", "127.0.0.1"),
        ]
        empty = self.replay(entry, until=2.5)
        self.expect_rendered(empty, key="tunnel-list-empty")
        self.expect_rendered(empty, key="tunnel-create")
        section = self._settled_rect(empty, "settings-section-tunnels")
        create_button = self._settled_rect(empty, "tunnel-create")
        notice = self._settled_rect(empty, "tunnel-list-empty")
        self.expect_rendered(empty, key="tunnel-supported-scope", within="tunnel-list-empty")
        self.assertAlmostEqual(create_button[1], section[1], delta=1.0)
        self.assertAlmostEqual(create_button[3], 28, delta=1.0)
        self.assertAlmostEqual(notice[1] - (create_button[1] + create_button[3]), 16, delta=1.0)
        self.assertGreaterEqual(create_button[0], section[0])
        self.assertAlmostEqual(create_button[0] + create_button[2],
                               section[0] + section[2], delta=1.0)
        self.assertFalse(path.exists(), "opening the list is read-only")
        invalid = self.replay([
            *prefix, *put(5.0, "new-conn-http-proxy-port", "0"),
            tap(6.0, "tunnel-edit-save"),
        ], until=7.0)
        dialog = self._settled_rect(invalid, "tunnel-edit-dialog")
        body = self._settled_rect(invalid, "tunnel-edit-dialog-body")
        port = self._settled_rect(invalid, "new-conn-http-proxy-port")
        self.assertGreaterEqual(port[1], body[1])
        self.assertLessEqual(port[1] + port[3], min(dialog[1] + dialog[3], body[1] + body[3]))
        self.expect_rendered(invalid, key="tunnel-edit-validation",
                             text="Enter a valid proxy port (1–65535).")
        self.assertFalse(path.exists(), "invalid definition was not written")

        create = self.replay([
            *prefix, *put(5.0, "new-conn-http-proxy-port", "18080"),
            tap(6.0, "new-conn-http-proxy-scheme-trigger"),
            tap(6.6, "new-conn-http-proxy-scheme-content/listbox/new-conn-http-proxy-scheme-scroll-area/scroll/rows/option1"),
            *put(7.4, "new-conn-http-proxy-username", "fixture-only"),
            *put(8.2, "new-conn-http-proxy-password", secret),
            tap(9.2, "tunnel-edit-save"),
        ], until=10.2)
        self.assertFalse(visible(create, key="tunnel-edit-dialog"))
        content = path.read_bytes()
        self.assertNotIn(secret.encode(), content)
        if os.name == "posix":
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        [record] = json.loads(content)
        tunnel_id = record["id"]
        self.assertEqual((record["name"], record["kind"]), ("Local proxy fixture", "httpProxy"))
        proxy = record["httpProxy"]
        self.assertEqual((proxy["host"], proxy["port"], proxy["scheme"], proxy["username"]),
                         ("127.0.0.1", 18080, "https", "fixture-only"))
        cipher = proxy["password"]
        self.assertGreaterEqual(len(base64.b64decode(cipher, validate=True)), 29)
        self.expect_rendered(create, key=f"tunnel-row-{tunnel_id}-name", text="Local proxy fixture")
        for operation in ("copy", "test", "delete"):
            self.assertFalse(visible(create, key=f"tunnel-{operation}-{tunnel_id}"))

        # The other real record type is listed but cannot be edited through
        # this route. Its public and unrecognized values survive HTTP edits.
        other = {"id": "legacy-other", "name": "Saved legacy SSH", "kind": "ssh",
                 "ssh": {"enabled": True, "host": "127.0.0.1"}, "future": {"preserve": [1, 2]}}
        path.write_text(json.dumps([record, other]), encoding="utf-8")
        edit_key = f"tunnel-edit-{tunnel_id}"
        reopened = self.replay([*entry, tap(2.4, edit_key)], until=3.6)
        self.expect_rendered(reopened, key="tunnel-password-state",
                             text="Leave the password blank to keep the saved password.")
        self.expect_rendered(reopened, text="HTTPS", within="new-conn-http-proxy-scheme-trigger")
        self.assertNotIn(secret, json.dumps(reopened, ensure_ascii=False))
        self.assertNotIn(cipher, json.dumps(reopened, ensure_ascii=False))

        edited = self.replay([
            *entry, tap(2.4, edit_key),
            *put(3.3, "tunnel-edit-name", "Renamed local proxy"),
            *put(4.3, "new-conn-http-proxy-port", "18443"),
            tap(5.4, "tunnel-edit-save"),
        ], until=6.3)
        current, untouched = json.loads(path.read_bytes())
        self.assertEqual(untouched, other)
        self.assertEqual((current["id"], current["name"]), (tunnel_id, "Renamed local proxy"))
        self.assertEqual((current["httpProxy"]["port"], current["httpProxy"]["password"]), (18443, cipher))
        self.expect_rendered(edited, key=f"tunnel-row-{tunnel_id}-name", text="Renamed local proxy")
        self.expect_rendered(edited, key="tunnel-row-legacy-other-name", text="Saved legacy SSH")
        self.assertFalse(visible(edited, key="tunnel-edit-legacy-other"))

        # Remove is below the body fold in this fixed-height modal.
        # Scroll it into view, then prove the removal is explicit and
        # still unsaved before saving the same user gesture sequence.
        clear_steps = [
            *entry, tap(2.4, edit_key),
            {"t": 3.25, "type": "wheel", "key": "tunnel-edit-dialog-body", "dy": -150},
            tap(3.9, "tunnel-clear-password"),
        ]
        armed = self.replay(clear_steps, until=4.55)
        self.expect_rendered(armed, key="tunnel-password-state",
                             text="Saving will remove the saved password unless you enter a new one.")
        self.assertEqual(json.loads(path.read_bytes())[0]["httpProxy"]["password"], cipher)
        cleared = self.replay([*clear_steps, tap(4.8, "tunnel-edit-save")], until=5.8)
        self.assertFalse(visible(cleared, key="tunnel-edit-dialog"))
        self.assertNotIn("password", json.loads(path.read_bytes())[0]["httpProxy"])
        self.assertEqual(settings.read_bytes(), saved_settings)
        self.assertFalse(self.db.exists(), "definition editing is not connection binding")

    def test_light_tunnel_edit_keeps_http_type_read_only_and_stacks_below_desktop_breakpoint(self):
        settings = self.root / "settings.json"
        initial = b'{"ui":{"language":"en","themeMode":"light"}}'
        settings.write_bytes(initial)
        store = self.root / "tunnels.json"
        opened = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-tunnels"),
                  tap(2.3, "tunnel-create")]

        for size, wide in (("1440x900", True), ("767x800", False)):
            tree = self.replay(opened, until=3.4, size=size)
            name = self._settled_rect(tree, "tunnel-edit-name-block")
            kind = self._settled_rect(tree, "tunnel-edit-kind-block")
            grid = self._settled_rect(tree, "tunnel-edit-identity")
            self.expect_rendered(tree, text="Type", within="tunnel-edit-kind-block")
            self.expect_rendered(tree, key="tunnel-edit-kind-readable", text="HTTP proxy")
            [fixed] = [e for e in tree["elements"] if e.get("key") == "tunnel-edit-kind-readable"]
            self.assertFalse(fixed["focusable"])
            self.assertEqual(fixed["role"], "text")
            self.assertFalse(visible(tree, key="tunnel-edit-kind-trigger"))
            if wide:
                self.assertAlmostEqual(kind[1], name[1], delta=.5)
                self.assertAlmostEqual(kind[0] - name[0] - name[2], 12, delta=.5)
                self.assertAlmostEqual(kind[2], name[2], delta=.5)
            else:
                self.assertAlmostEqual(kind[0], name[0], delta=.5)
                self.assertAlmostEqual(kind[1] - name[1] - name[3], 12, delta=.5)
                self.assertAlmostEqual(kind[2], grid[2], delta=.5)
            self.assertAlmostEqual(kind[0] + kind[2], grid[0] + grid[2], delta=.5)

        edited = [*opened, tap(3.5, "tunnel-edit-name"),
                  {"t": 3.8, "type": "paste", "text": "Unsaved Light fixture"}]
        cancelled = self.replay([*edited, tap(4.6, "tunnel-edit-cancel")], until=5.5)
        self.assertFalse(visible(cancelled, key="tunnel-edit-dialog"))
        self.expect_rendered(cancelled, key="tunnel-list-empty")
        self.assertFalse(store.exists(), "opening, editing and cancelling makes no registry")
        self.assertEqual(settings.read_bytes(), initial)

    def test_malformed_saved_tunnel_file_shows_real_error_and_cannot_be_overwritten(self):
        path = self.root / "tunnels.json"
        malformed = b'{"unexpected":'
        path.write_bytes(malformed)
        settings = self.root / "settings.json"
        settings.write_text('{"ui":{"language":"en","themeMode":"dark"}}', encoding="utf-8")
        view = self.replay([
            tap(.5, "rail-settings"), tap(1.4, "settings-nav-tunnels"),
            tap(2.5, "tunnel-retry"),
        ], until=3.5)
        self.expect_rendered(view, key="tunnel-load-error")
        self.assertFalse(visible(view, key="tunnel-list-empty"))
        self.assertFalse(visible(view, key="tunnel-create"))
        self.assertEqual(path.read_bytes(), malformed)
