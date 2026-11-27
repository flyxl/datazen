//! Mount the real preset through the stock settings port and a disposable profile.
use std::{fs, path::Path, sync::Arc};

use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{app_poll, start_shared_app_state, view, AppState};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

const SELECT: &str = "settings-page-size-preset-trigger";
const MENU: &str = "settings-page-size-preset-content";

fn runtime(data: &Path, dark: bool) -> Runtime<AppState> {
    let repository = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    let adapter = LiveAdapter::with_data_dir(Arc::new(repository), data);
    let mut rt = Runtime::new(
        start_shared_app_state(Some(Box::new(adapter))),
        view, (1440.0, 900.0), Env { system_dark: !dark, scale: 1.0 },
    );
    rt.poll = Some(app_poll);
    rt.frame(0.0);
    rt
}

fn settled(rt: &mut Runtime<AppState>) {
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    settled(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: missing live hit"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    settled(rt);
}

fn element(rt: &Runtime<AppState>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key}: missing mounted node"))
}

fn page(rt: &mut Runtime<AppState>) {
    press(rt, "rail-settings");
    press(rt, "settings-nav-dataBrowsing");
    assert_eq!(rt.state.settings_section(), "dataBrowsing");
}

fn has_value(rt: &Runtime<AppState>, text: &str) {
    let trigger = element(rt, SELECT);
    assert!(rt.elements().iter().any(|e| !e.exiting
        && e.path.starts_with(&format!("{}/", trigger.path))
        && e.text.as_deref() == Some(text)), "preset does not display {text:?}");
}

#[test]
fn default_page_size_edge_keeps_the_real_presets_cancel_and_profile_write() {
    for (dark, theme) in [(false, "light"), (true, "dark")] {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        let original = json!({
            "ui": { "language": "en", "themeMode": theme, "tablePageSize": 50, "queryMaxRows": 123 },
        });
        let before = serde_json::to_vec(&original).unwrap();
        fs::write(&path, &before).unwrap();

        let mut rt = runtime(profile.path(), dark);
        page(&mut rt);
        assert_eq!(rt.state.dark_theme(), dark);
        let field = element(&rt, SELECT);
        assert_eq!(field.rect, [650.0, 103.5, 496.0, 36.0]);
        assert_eq!(field.fill.hex(), if dark { "#0a0d14ff" } else { "#ffffffff" });
        assert_eq!(field.role, "combobox");
        assert_eq!(field.label.as_deref(), Some("Default Page Size"));
        assert!(field.focusable);
        has_value(&rt, "50 rows");

        for key in ["settings-page-size-input", "settings-page-size-set",
                    "settings-max-rows-input", "settings-max-rows-set",
                    "settings-data-tools", "settings-data-unsupported"] {
            let _ = element(&rt, key);
        }

        press(&mut rt, SELECT);
        let _ = element(&rt, MENU);
        assert_eq!(element(&rt, SELECT).expanded, Some(true));
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settled(&mut rt);
        assert!(rt.find(MENU).is_none());
        assert_eq!(fs::read(&path).unwrap(), before, "Esc does not write");
        has_value(&rt, "50 rows");

        press(&mut rt, SELECT);
        let menu = element(&rt, MENU);
        let prefix = format!("{}/", menu.path);
        let choice = rt.elements().into_iter().find(|e| !e.exiting
            && e.role == "option" && e.path.starts_with(&prefix)
            && e.label.as_deref() == Some("100 rows")).expect("real preset option");
        press(&mut rt, &choice.path);
        assert!(rt.find(MENU).is_none());
        has_value(&rt, "100 rows");
        let mut expected = original.clone();
        expected["ui"]["tablePageSize"] = json!(100);
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);

        drop(rt);
        let mut reopened = runtime(profile.path(), dark);
        page(&mut reopened);
        has_value(&reopened, "100 rows");
    }
}
