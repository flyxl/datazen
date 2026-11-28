//! Real Settings controls over the persisted native adapter and throwaway data roots.
use std::{fs, path::Path, sync::Arc};

use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{app_poll, start_shared_app_state, view, AppState};
use datazen_ui_api::types::LogLevel;
use rune::{ElementInfo, Env, Runtime};
use serde_json::{json, Value};

const TRIGGER: &str = "settings-log-level-trigger";

fn runtime(data: &Path) -> Runtime<AppState> {
    let repository = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    let adapter = LiveAdapter::with_data_dir(Arc::new(repository), data);
    let mut rt = Runtime::new(start_shared_app_state(Some(Box::new(adapter))), view,
        (1440.0, 900.0), Env { system_dark: false, scale: 1.0 });
    rt.poll = Some(app_poll);
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    rt.settle_all();
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: missing live hit"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
}

fn page(rt: &mut Runtime<AppState>) {
    press(rt, "rail-settings");
    press(rt, "settings-nav-logging");
    assert_eq!(rt.state.settings_section(), "logging");
}

fn control(rt: &Runtime<AppState>) -> ElementInfo {
    rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(TRIGGER))
        .expect("real logging selector")
}

fn assert_value(rt: &Runtime<AppState>, label: &str) {
    let trigger = control(rt);
    assert_eq!(trigger.role, "combobox");
    assert!(trigger.focusable);
    assert!(rt.elements().iter().any(|e| !e.exiting
        && e.path.starts_with(&format!("{}/", trigger.path))
        && e.text.as_deref() == Some(label)), "selector did not display {label}");
}

fn choose(rt: &mut Runtime<AppState>, label: &str) {
    press(rt, TRIGGER);
    let root = rt.elements().into_iter().find(|e| !e.exiting
        && e.key.as_deref() == Some("settings-log-level-content")).expect("select opened");
    let choice = rt.elements().into_iter().find(|e| !e.exiting && e.role == "option"
        && e.path.starts_with(&format!("{}/", root.path))
        && e.label.as_deref() == Some(label)).expect("real Log Level option");
    press(rt, &choice.path);
    assert!(rt.find("settings-log-level-content").is_none());
}

#[test]
fn native_log_level_persists_only_its_field_and_reloads_in_a_fresh_adapter() {
    let profile = tempfile::tempdir().unwrap();
    let path = profile.path().join("settings.json");
    let original = json!({"ui":{"language":"en","tablePageSize":33},"unknown":[1,2]});
    fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let mut rt = runtime(profile.path());
    page(&mut rt);
    assert_eq!(rt.state.desktop.native_log_level(), Some(LogLevel::Info));
    assert_value(&rt, "Info");
    let field = control(&rt);
    let content = rt.elements().into_iter()
        .find(|el| !el.exiting && el.key.as_deref() == Some("settings-log-level-row-content"))
        .expect("logging control column");
    assert_eq!(field.rect, content.rect, "use the complete responsive value column");
    assert_eq!(&field.rect[2..], &[496.0, 36.0], "reference 1440px layout");
    for key in ["settings-log-path-row", "settings-view-logs-button", "data-cleanup-section"] {
        assert!(rt.find(key).is_none(), "{key} is not a native Logging capability");
    }
    choose(&mut rt, "Debug");
    assert_value(&rt, "Debug");
    let mut expected = original;
    expected["logLevel"] = json!("debug");
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);
    drop(rt);

    let mut restarted = runtime(profile.path());
    page(&mut restarted);
    assert_eq!(restarted.state.desktop.native_log_level(), Some(LogLevel::Debug));
    assert_value(&restarted, "Debug");
}

#[test]
fn native_log_level_cannot_replace_a_broken_or_unwritable_saved_snapshot() {
    let profile = tempfile::tempdir().unwrap();
    let path = profile.path().join("settings.json");
    let intact = br#"{"logLevel":"warn","ui":{"language":"en"}}"#;
    fs::write(&path, intact).unwrap();
    let mut rt = runtime(profile.path());
    page(&mut rt);
    assert_value(&rt, "Warn");

    // Simulate a bad external change while the app still has the last good cache.
    for corrupted in [b"{ broken".as_slice(), b"  ", b"[]"] {
        fs::write(&path, corrupted).unwrap();
        choose(&mut rt, "Debug");
        assert_eq!(fs::read(&path).unwrap(), corrupted);
        assert_value(&rt, "Warn");
        assert!(rt.find("settings-log-level-error").is_some());
    }
    fs::write(&path, intact).unwrap();
    // The existing atomic repository uses a sibling .json.tmp; a
    // directory there prevents a write without changing profile ACLs.
    fs::create_dir(path.with_extension("json.tmp")).unwrap();
    choose(&mut rt, "Debug");
    assert_eq!(fs::read(&path).unwrap(), intact);
    assert_value(&rt, "Warn");
    assert!(rt.find("settings-log-level-error").is_some());
}

#[test]
fn normal_native_process_uses_the_profile_level_on_stderr_without_a_gui_fixture() {
    use std::process::Command;

    let profile = tempfile::tempdir().unwrap();
    let connections = profile.path().join("connections.json");
    let settings = profile.path().join("settings.json");
    // A real and deterministic early GUI startup failure keeps the process
    // from starting a renderer. No production test argument or probe path.
    fs::write(&connections, "{ unreadable").unwrap();
    for (level, expect_debug) in [("warn", false), ("debug", true)] {
        fs::write(&settings, serde_json::to_vec(&json!({"logLevel": level})).unwrap()).unwrap();
        let run = Command::new(env!("CARGO_BIN_EXE_datazen-native"))
            .env_remove("RUST_LOG")
            .env("DATAZEN_NATIVE_CONNECTIONS", &connections)
            .output().expect("normal native binary launch");
        assert_eq!(run.status.code(), Some(1));
        assert!(run.stdout.is_empty());
        let stderr = String::from_utf8(run.stderr).unwrap();
        assert!(stderr.contains("the connection store did not open"), "{stderr}");
        assert_eq!(stderr.contains("Opening the native profile"), expect_debug, "{stderr}");
        assert!(!stderr.contains("Native GUI is starting"), "did not start the GUI");
    }
}
