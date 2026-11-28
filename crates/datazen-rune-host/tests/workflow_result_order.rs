//! Mount actual Settings/Workflow controls against a fresh stock LiveAdapter.
use std::{fs, path::Path, sync::Arc};

use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{app_poll, start_shared_app_state, view, AppState};
use datazen_ui_api::types::WorkflowStepResultOrder::{self, Asc, Desc};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

const SELECT: &str = "settings-workflow-step-order-trigger";
const MENU: &str = "settings-workflow-step-order-content";
const TOGGLE: &str = "workflow-step-order-toggle";
const ASC_LABEL: &str = "Step ascending (first step first)";
const DESC_LABEL: &str = "Step descending (last step first)";

fn runtime(data: &Path, dark: bool) -> Runtime<AppState> {
    let repository = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    let adapter = LiveAdapter::with_data_dir(Arc::new(repository), data);
    let mut rt = Runtime::new(
        start_shared_app_state(Some(Box::new(adapter))),
        view,
        (1440.0, 900.0),
        Env {
            system_dark: !dark,
            scale: 1.0,
        },
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

fn settings(rt: &mut Runtime<AppState>) {
    press(rt, "rail-settings");
    press(rt, "settings-nav-dataBrowsing");
    assert_eq!(rt.state.settings_section(), "dataBrowsing");
}

fn has_choice(rt: &Runtime<AppState>, label: &str) {
    let trigger = element(rt, SELECT);
    assert_eq!(trigger.role, "combobox");
    assert_eq!(trigger.label.as_deref(), Some("Workflow step result order"));
    assert!(trigger.focusable);
    assert!(rt.elements().iter().any(|e| !e.exiting
        && e.path.starts_with(&format!("{}/", trigger.path))
        && e.text.as_deref() == Some(label)), "choice does not display {label:?}");
}

fn choose(rt: &mut Runtime<AppState>, label: &str) {
    press(rt, SELECT);
    let trigger = element(rt, SELECT);
    assert_eq!(trigger.expanded, Some(true));
    let prefix = format!("{}/", element(rt, MENU).path);
    let option = rt.elements().into_iter().find(|e| !e.exiting
        && e.role == "option" && e.path.starts_with(&prefix)
        && e.label.as_deref() == Some(label)).expect("real option");
    press(rt, &option.path);
    assert!(rt.find(MENU).is_none());
}

fn open_report(rt: &mut Runtime<AppState>) {
    press(rt, "rail-workflows");
    press(rt, "workflow-side-tab-history");
    press(rt, "workflow-history-list-item-order-fixture");
}

fn check_report(rt: &Runtime<AppState>, order: WorkflowStepResultOrder, selected: &str) {
    assert_eq!(rt.state.workflow_step_order(), order);
    let first = element(rt, "workflow-step-read_first").rect[0];
    let second = element(rt, "workflow-step-read_second").rect[0];
    match order {
        Asc => assert!(first < second, "{first:?} should precede {second:?}"),
        Desc => assert!(second < first, "{second:?} should precede {first:?}"),
    }
    assert_eq!(element(rt, "workflow-step-detail-id").text.as_deref(), Some(selected));
    let label = element(rt, "workflow-step-order-toggle-label");
    assert_eq!(label.text.as_deref(), Some(match order { Asc => "Oldest first", Desc => "Latest first" }));
}

#[test]
fn mounted_settings_and_report_share_one_persisted_order_without_reordering_history_or_selection() {
    for (dark, theme) in [(false, "light"), (true, "dark")] {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        let history = profile.path().join("workflow-run-history.json");
        let before = json!({
            "ui": { "language": "en", "themeMode": theme, "tablePageSize": 50, "queryMaxRows": 123 },
            "unrelated": { "value": true },
        });
        let legacy = serde_json::to_vec(&before).unwrap();
        fs::write(&path, &legacy).unwrap();

        // This mounted test uses a known persisted history fixture, not a
        // synthetic panel event. The separate stock CLI plan executes SQL.
        let report = serde_json::to_vec(&json!([{
            "id":"order-fixture", "workflowId":"order-flow", "workflowName":"Order flow",
            "variables":{}, "createdAt":"2026-01-01T00:00:00Z",
            "result": {
                "success":true,"finalOutput":"","totalTimeMs":3,
                "steps":[
                    { "stepId":"read_first","stepType":"query","status":"success","executionTimeMs":1,"sqlExecuted":"SELECT 1" },
                    { "stepId":"read_second","stepType":"query","status":"success","executionTimeMs":2,"sqlExecuted":"SELECT 2" }
                ]
            }
        }])).unwrap();
        fs::write(&history, &report).unwrap();

        let mut rt = runtime(profile.path(), dark);
        assert_eq!(rt.state.dark_theme(), dark);
        open_report(&mut rt);
        check_report(&rt, Desc, "read_second");
        press(&mut rt, "workflow-step-read_first");
        check_report(&rt, Desc, "read_first");

        settings(&mut rt);
        has_choice(&rt, DESC_LABEL);
        press(&mut rt, SELECT);
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settled(&mut rt);
        assert!(rt.find(MENU).is_none());
        assert_eq!(fs::read(&path).unwrap(), legacy, "no default migration");
        choose(&mut rt, DESC_LABEL);
        assert_eq!(fs::read(&path).unwrap(), legacy, "saying no change does not write");

        choose(&mut rt, ASC_LABEL);
        has_choice(&rt, ASC_LABEL);
        let mut expected = before;
        expected["workflowStepResultOrder"] = json!("asc");
        let ascending = fs::read(&path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&ascending).unwrap(), expected);
        press(&mut rt, "settings-back");
        check_report(&rt, Asc, "read_first");

        // Force the actual atomic write to fail, independently at each UI
        // entry point. Visible order and prior byte-for-byte profile remain.
        let temp_collision = path.with_extension("json.tmp");
        fs::create_dir(&temp_collision).unwrap();
        press(&mut rt, TOGGLE);
        let _ = element(&rt, "workflow-operation-error");
        check_report(&rt, Asc, "read_first");
        settings(&mut rt);
        choose(&mut rt, DESC_LABEL);
        let _ = element(&rt, "settings-error");
        has_choice(&rt, ASC_LABEL);
        assert_eq!(fs::read(&path).unwrap(), ascending);
        fs::remove_dir(temp_collision).unwrap();

        drop(rt);
        let mut reopened = runtime(profile.path(), dark);
        open_report(&mut reopened);
        check_report(&reopened, Asc, "read_first");
        press(&mut reopened, "workflow-step-read_second");
        press(&mut reopened, TOGGLE);
        check_report(&reopened, Desc, "read_second");
        expected["workflowStepResultOrder"] = json!("desc");
        let descending = fs::read(&path).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&descending).unwrap(), expected);
        drop(reopened);

        let mut reopened = runtime(profile.path(), dark);
        settings(&mut reopened);
        has_choice(&reopened, DESC_LABEL);
        assert_eq!(fs::read(&path).unwrap(), descending);
        assert_eq!(fs::read(&history).unwrap(), report,
            "presentation changes never rewrite the saved execution order or report");
    }
}

#[test]
fn no_read_or_failed_write_changes_legacy_or_corrupt_documents() {
    use datazen_ui_api::DesktopAdapter;

    let profile = tempfile::tempdir().unwrap();
    let path = profile.path().join("settings.json");
    let repository = Arc::new(FileConnectionRepository::open(&profile.path().join("connections.json")).unwrap());
    let mut adapter = LiveAdapter::with_data_dir(repository.clone(), profile.path());
    assert_eq!(adapter.read_workflow_step_result_order().unwrap(), Desc);
    assert!(!path.exists(), "first read is not a write");
    fs::write(&path, br#"{"workflowStepResultOrder":"asc","unrelated":1}"#).unwrap();
    drop(adapter);
    adapter = LiveAdapter::with_data_dir(repository.clone(), profile.path());
    assert_eq!(adapter.read_workflow_step_result_order().unwrap(), Asc);
    // A corrupted file encountered after boot must not be reset to the
    // cached valid document, and the old visible value remains available.
    fs::write(&path, b"{ this is corrupt").unwrap();
    assert!(adapter.write_workflow_step_result_order(Desc).is_err());
    assert_eq!(adapter.read_workflow_step_result_order().unwrap(), Asc);
    assert_eq!(fs::read(&path).unwrap(), b"{ this is corrupt");
    drop(adapter);
    let unknown = br#"{"workflowStepResultOrder":"sideways","ui":{"workflowStepResultOrder":"asc"}}"#;
    fs::write(&path, unknown).unwrap();
    let adapter = LiveAdapter::with_data_dir(repository, profile.path());
    assert_eq!(adapter.read_workflow_step_result_order().unwrap(), Desc);
    assert_eq!(fs::read(&path).unwrap(), unknown, "no migration on read");
}

#[test]
fn workflow_order_has_separate_wrapping_label_hint_and_select_in_the_live_host() {
    const ROW: &str = "settings-workflow-step-order-row";
    const EN_HINT: &str = "Order of the step result tabs shown after a workflow run.";
    const ZH_HINT: &str = "工作流运行后步骤结果选项卡的排列顺序。";
    for (locale, theme, dark, help, ascending) in [
        ("en", "light", false, EN_HINT, ASC_LABEL),
        ("zh-CN", "dark", true, ZH_HINT, "步骤递增（第一步在前）"),
    ] {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        let before = json!({"ui": {"language": locale, "themeMode": theme}});
        let before_bytes = serde_json::to_vec(&before).unwrap();
        fs::write(&path, &before_bytes).unwrap();
        let mut rt = runtime(profile.path(), dark);
        settings(&mut rt);

        let slot = element(&rt, &format!("{ROW}-label")).rect;
        let label = element(&rt, &format!("{ROW}-label-text")).rect;
        let help_mark = element(&rt, &format!("{ROW}-hint-mark"));
        let mark = help_mark.rect;
        let target = element(&rt, SELECT).rect;
        let prior = element(&rt, "settings-limit-select-results-row").rect;
        assert_eq!(slot[2], 160.0);
        assert_eq!([mark[2], mark[3]], [16.0, 16.0]);
        assert!(label[0] + label[2] + 7.9 <= mark[0], "hint overlaps the label: {label:?}, {mark:?}");
        assert!(mark[0] + mark[2] <= slot[0] + slot[2] + 0.1, "hint left its label column");
        assert!((target[0] - slot[0] - slot[2] - 16.0).abs() < 0.1, "Select gutter changed");
        assert!((target[1] - prior[1] - prior[3] - 20.0).abs() < 0.1, "Select row y moved");
        assert_eq!([target[2], target[3]], [496.0, 36.0]);
        if locale == "en" {
            assert!(label[3] >= 39.5, "the long English label did not wrap: {label:?}");
        }

        // Move to the actual independent 16px hit target. Before the fix
        // this point was inside the Select, and the shared mark had no
        // automatic tooltip behavior.
        assert_eq!(help_mark.help.as_deref(), Some(help));
        rt.pointer_move(mark[0] + mark[2] / 2.0, mark[1] + mark[3] / 2.0);
        for _ in 0..90 {
            rt.frame(1.0 / 60.0);
        }
        let (tooltip, bounds) = rt.help_tooltip().expect("hover opens the real native help");
        assert_eq!(tooltip, help);
        assert!(bounds[1] + bounds[3] < mark[1], "native help is above the mark");
        assert!(rt.find(MENU).is_none(), "hover did not open the Select");
        assert_eq!(fs::read(&path).unwrap(), before_bytes, "hover did not write preferences");

        rt.pointer_move(1120.0, target[1] + target[3] / 2.0);
        settled(&mut rt);
        assert!(rt.help_tooltip().is_none());
        choose(&mut rt, ascending);
        assert_eq!(rt.state.workflow_step_order(), Asc);
        let mut expected = before;
        expected["workflowStepResultOrder"] = json!("asc");
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(), expected);
    }
}
