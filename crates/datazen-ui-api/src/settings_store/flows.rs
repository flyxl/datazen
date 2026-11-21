use serde_json::{json, Value};

use super::normalize::{
    is_unknown_version_value_presentation, next_value_presentation_revision,
    normalize_loaded_settings, normalize_settings_language, parse_stored_value_presentation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadRecovery {
    RetryWithoutPack,
    Fail,
}

pub fn load_fence(current: &mut u64) -> u64 {
    *current = current.wrapping_add(1);
    *current
}

pub fn load_fence_current(generation: u64, current: u64) -> bool {
    generation == current
}

pub fn theme_recovery(pack_id: Option<&str>) -> LoadRecovery {
    match pack_id {
        Some(pack) if !pack.is_empty() => LoadRecovery::RetryWithoutPack,
        _ => LoadRecovery::Fail,
    }
}

pub fn persist_merge(next: &Value, saved: Option<&Value>) -> Value {
    let mut merged = next.clone();
    if let Some(Value::Object(saved)) = saved {
        if let Value::Object(target) = &mut merged {
            for (key, value) in saved {
                target.insert(key.clone(), value.clone());
            }
        }
    }
    let language = merged
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    merged["language"] = json!(normalize_settings_language(language));
    let presentation = merged.get("valuePresentation");
    merged["valuePresentation"] = parse_stored_value_presentation(presentation);
    merged
}

pub fn update_settings_merge(current: &Value, partial: &Value) -> Value {
    let mut merged = current.clone();
    if let (Value::Object(target), Value::Object(changes)) = (&mut merged, partial) {
        for (key, value) in changes {
            target.insert(key.clone(), value.clone());
        }
    }
    let language = merged
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    merged["language"] = json!(normalize_settings_language(language));
    if let Some(theme) = partial.get("theme") {
        merged["theme"] = theme.clone();
    }
    merged
}

pub fn authoritative_read(raw: Option<&Value>) -> Option<(Value, Value)> {
    let raw = raw?;
    let object = raw.as_object()?;
    let raw_presentation = object.get("valuePresentation")?;
    Some((normalize_loaded_settings(raw), raw_presentation.clone()))
}

pub fn value_presentation_fresh(authoritative_raw: Option<&Value>, base: &Value) -> Value {
    if is_unknown_version_value_presentation(authoritative_raw) {
        authoritative_raw
            .cloned()
            .unwrap_or_else(|| parse_stored_value_presentation(Some(base)))
    } else {
        parse_stored_value_presentation(Some(base))
    }
}

pub fn settings_changed(previous: &Value, incoming: &Value, revision: u64) -> (Value, u64, bool) {
    let language = incoming
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    let presentation = parse_stored_value_presentation(incoming.get("valuePresentation"));
    let mut normalized = incoming.clone();
    normalized["language"] = json!(normalize_settings_language(language));
    normalized["valuePresentation"] = presentation.clone();
    let next_revision = next_value_presentation_revision(
        &parse_stored_value_presentation(previous.get("valuePresentation")),
        &presentation,
        revision,
    );
    let theme_changed = previous.get("theme") != incoming.get("theme");
    (normalized, next_revision, theme_changed)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn generation_fence_drops_stale_responses() {
        let mut generation = 2;
        assert_eq!(load_fence(&mut generation), 3);
        assert!(load_fence_current(3, generation));
        assert!(!load_fence_current(2, generation));
    }

    #[test]
    fn theme_recovery_only_retries_with_a_pack() {
        assert_eq!(theme_recovery(Some("p1")), LoadRecovery::RetryWithoutPack);
        assert_eq!(theme_recovery(Some("")), LoadRecovery::Fail);
        assert_eq!(theme_recovery(None), LoadRecovery::Fail);
    }

    #[test]
    fn persist_merge_prefers_the_backend_response() {
        let saved = persist_merge(
            &json!({"language": "en_US", "valuePresentation": null}),
            Some(&json!({"language": "zh", "extra": 1})),
        );
        assert_eq!(saved["language"], "zh-CN");
        assert_eq!(saved["extra"], 1);
        assert_eq!(saved["valuePresentation"]["version"], 1);
    }

    #[test]
    fn update_merge_normalizes_language_and_swaps_theme_on_request() {
        let merged = update_settings_merge(
            &json!({"language": "en", "theme": {"mode": "dark"}}),
            &json!({"language": "zh_CN", "theme": {"mode": "light"}}),
        );
        assert_eq!(merged["language"], "zh-CN");
        assert_eq!(merged["theme"]["mode"], "light");

        let no_theme = update_settings_merge(
            &json!({"language": "en", "theme": {"mode": "dark"}}),
            &json!({"safeMode": false}),
        );
        assert_eq!(no_theme["theme"]["mode"], "dark");
    }

    #[test]
    fn authoritative_read_requires_an_object_with_value_presentation() {
        assert_eq!(authoritative_read(None), None);
        assert_eq!(authoritative_read(Some(&json!([]))), None);
        assert_eq!(authoritative_read(Some(&json!({"language": "en"}))), None);
        let read = authoritative_read(Some(&json!({
            "language": "zh", "valuePresentation": {"version": 1, "renderers": {}}
        })))
        .unwrap();
        assert_eq!(read.0["language"], "zh-CN");
        assert_eq!(read.1["version"], 1);
    }

    #[test]
    fn unknown_version_uses_the_raw_payload_as_the_fresh_basis() {
        let raw = json!({"version": 2, "rules": [1], "columnOverrides": {"a": 1}});
        let fresh = value_presentation_fresh(Some(&raw), &json!({"version": 1}));
        assert_eq!(fresh, raw);
        let known = value_presentation_fresh(Some(&json!({"version": 1})), &json!({"version": 1}));
        assert_eq!(known["version"], 1);
    }

    #[test]
    fn local_apply_detects_theme_changes_and_bumps_revision() {
        let previous = json!({"theme": {"mode": "dark"}, "valuePresentation": {"version": 1}});
        let incoming = json!({
            "theme": {"mode": "light"},
            "language": "en_US",
            "valuePresentation": {"version": 2}
        });
        let (settings, revision, changed) = settings_changed(&previous, &incoming, 4);
        assert!(changed);
        assert_eq!(revision, 5);
        assert_eq!(settings["language"], "en");
        assert_eq!(settings["valuePresentation"]["version"], 2);

        let unchanged = settings_changed(&previous, &previous, 4);
        assert!(!unchanged.2);
        assert_eq!(unchanged.1, 4);
    }
}
