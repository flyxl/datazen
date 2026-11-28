//! Real mounted Settings switch, backed by the stock disposable settings store.
use std::{fs, path::Path, sync::Arc};

use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{start_shared_app_state, view, AppState};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

const SWITCH: &str = "settings-grid-alternating-rows";

fn settled(rt: &mut Runtime<AppState>) {
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing live {key}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    settled(rt);
}

fn find(rt: &Runtime<AppState>, key: &str) -> ElementInfo {
    rt.elements().into_iter().find(|el| !el.exiting && el.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing mounted {key}"))
}

fn open(data: &Path, dark: bool) -> Runtime<AppState> {
    let repo = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    let adapter = LiveAdapter::with_data_dir(Arc::new(repo), data);
    let mut rt = Runtime::new(start_shared_app_state(Some(Box::new(adapter))),
        view, (1440.0, 900.0), Env {system_dark: !dark, scale: 1.0});
    rt.set_full_keyboard_access(true);
    rt.frame(0.0);
    press(&mut rt, "rail-settings");
    press(&mut rt, "settings-nav-dataBrowsing");
    assert_eq!(rt.state.settings_section(), "dataBrowsing");
    assert_eq!(rt.state.dark_theme(), dark);
    rt
}

fn assert_switch(rt: &Runtime<AppState>, enabled: bool) {
    let item = find(rt, SWITCH);
    assert_eq!(item.role, "switch");
    assert_eq!(item.label.as_deref(), Some("Alternating row backgrounds"));
    let c = &datazen_ui::ui::kit::kit(rt.state.dark_theme()).c;
    assert_eq!(item.fill, if enabled { c.bg_accent } else { c.border_base });
    assert_eq!(item.rect[2..], [36.0, 20.0]);
    assert!(item.focusable);
}

#[test]
fn live_grid_striping_switch_uses_exact_key_and_rejects_failed_atomic_write() {
    for (dark, theme) in [(false, "light"), (true, "dark")] {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        let original = json!({
            "ui": {"language":"en", "themeMode":theme, "tablePageSize":50},
            "defaultTableSort":"primaryKeyAsc","rowCountMode":"exact","unrelated":{"pin":1}
        });
        let initial = serde_json::to_vec(&original).unwrap();
        fs::write(&path, &initial).unwrap();

        let mut rt = open(profile.path(), dark);
        assert_switch(&rt, true);
        assert_eq!(fs::read(&path).unwrap(), initial, "mount does not seed default");
        let order = ["settings-page-size-preset-trigger", "settings-row-count-mode-trigger",
            "settings-default-table-sort-trigger", SWITCH, "settings-max-rows-input",
            "settings-workflow-step-order-trigger", "settings-data-tools"];
        for near in order.windows(2) {
            assert!(find(&rt, near[0]).rect[1] < find(&rt, near[1]).rect[1]);
        }

        press(&mut rt, SWITCH);
        assert_switch(&rt, false);
        let mut expected = original.clone();
        expected["gridAlternatingRows"] = json!(false);
        let disabled = fs::read(&path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&disabled).unwrap(), expected);

        // Atomic store failure cannot flip the visible/cached value.
        let collision = path.with_extension("json.tmp");
        fs::create_dir(&collision).unwrap();
        press(&mut rt, SWITCH);
        assert_switch(&rt, false);
        let prefix = format!("{}/", find(&rt, "settings-error").path);
        assert!(rt.elements().iter().any(|e| !e.exiting && e.path.starts_with(&prefix)
            && e.text.as_deref() == Some("Could not save alternating row backgrounds.")));
        assert_eq!(fs::read(&path).unwrap(), disabled);
        fs::remove_dir(collision).unwrap();

        drop(rt);
        let mut reopened = open(profile.path(), dark);
        assert_switch(&reopened, false);
        for _ in 0..100 {
            if find(&reopened, SWITCH).focused { break; }
            reopened.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
            reopened.frame(1.0 / 60.0);
        }
        assert!(find(&reopened, SWITCH).focused);
        reopened.key(KeyEvent { key: Key::Text(" ".into()), mods: Mods::default() });
        settled(&mut reopened);
        assert_switch(&reopened, true);
        expected["gridAlternatingRows"] = json!(true);
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);

        // A current malformed document must never be overwritten by the valid boot cache.
        let corrupt = b"{ incomplete";
        fs::write(&path, corrupt).unwrap();
        press(&mut reopened, SWITCH);
        assert_switch(&reopened, true);
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}
