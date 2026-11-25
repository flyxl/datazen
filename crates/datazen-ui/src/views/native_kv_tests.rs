use super::*;
use datazen_ui_api::native_kv::KvPage;

fn workbench(generation: u32) -> KvWorkbench {
    KvWorkbench::new(NativeKvDriver::Redis, "redis-profile".into(), generation)
}

fn reply(preview: &str) -> Result<KvReply, String> {
    Ok(KvReply::Detail(KvDetail {
        key_type: "string".into(),
        ttl: -1,
        preview: preview.into(),
        string_edit: None,
        hash_fields: Vec::new(),
        collection_edits: Vec::new(),
    }))
}

#[test]
fn old_selection_and_reconnected_session_cannot_overwrite_detail() {
    let mut vm = workbench(3);
    let first = vm.select("old".into());
    let second = vm.select("current".into());
    let mut previous_session = second.clone();
    previous_session.session_generation = 2;
    vm.receive(previous_session, reply("wrong session"));
    vm.receive(first.clone(), reply("wrong selection"));
    assert!(vm.detail.is_none());
    assert_eq!(vm.detail_request, Some(second.request_id));
    vm.receive(second, reply("correct"));
    vm.receive(first, reply("late"));
    assert_eq!(vm.detail.expect("current value").preview, "correct");
}

#[test]
fn dirty_redis_draft_only_clears_after_confirmed_save() {
    let mut vm = workbench(3);
    vm.writable = true;
    vm.selected = Some("current".into());
    vm.begin_edit(EditMode::String, "original".into());
    vm.draft.as_mut().expect("draft").value.text = "replacement".into();
    assert!(!vm.can_leave());
    let save = vm.submit().expect("write request");
    assert!(!vm.can_leave());
    assert!(vm.receive(save, Err("write rejected".into())).is_none());
    assert_eq!(
        vm.draft.as_ref().expect("draft persists").value.text,
        "replacement"
    );
    assert!(!vm.can_leave());
    let retry = vm.submit().expect("retry");
    assert!(vm.receive(retry, Ok(KvReply::Edited)).is_some());
    assert!(vm.draft.is_none());
}

#[test]
fn ttl_rejects_delete_values_and_allows_expiry_removal() {
    let mut draft = Draft {
        mode: EditMode::Ttl("string".into()),
        original: "100".into(),
        value: TextEdit::new("0"),
    };
    assert!(draft.change().is_err());
    draft.value.text = "-2".into();
    assert!(draft.change().is_err());
    draft.value.text = "-1".into();
    assert_eq!(
        draft.change(),
        Ok(KvChange::Ttl {
            expected_type: "string".into(),
            seconds: -1
        })
    );
}

#[test]
fn empty_scan_page_is_not_completion_and_repeated_keys_are_deduplicated() {
    let mut vm = workbench(3);
    let first = vm.next_page().expect("initial scan");
    vm.receive(
        first,
        Ok(KvReply::Page(KvPage {
            cursor: 24,
            keys: vec![],
            truncated: true,
            writable: false,
            db_index: 0,
            databases: vec![0],
            cluster: false,
        })),
    );
    let second = vm.next_page().expect("empty page kept the cursor");
    assert_eq!(
        second.kind,
        KvReadKind::Scan {
            cursor: 24,
            pattern: "*".into(),
            key_type: RedisKeyType::Any,
            persistent: false
        }
    );
    let key = KvKey {
        key: "k".into(),
        key_type: "string".into(),
        ttl: -1,
    };
    vm.receive(
        second,
        Ok(KvReply::Page(KvPage {
            cursor: 0,
            keys: vec![key.clone(), key],
            truncated: false,
            writable: false,
            db_index: 0,
            databases: vec![0],
            cluster: false,
        })),
    );
    assert_eq!(vm.keys.len(), 1);
    assert!(vm.next_page().is_none());
}
