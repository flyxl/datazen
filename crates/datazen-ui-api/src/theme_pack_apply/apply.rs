use super::ids::{parse_wapp_theme_pack_id, THEME_PACK_CHANGED_EVENT};

pub const THEME_PACK_APPLY_GAP: [&str; 8] = [
    "style element injection by id",
    "getComputedStyle background sync",
    "URL.createObjectURL / revokeObjectURL",
    "wappCommands.readWappFile / getWappManifest",
    "setActiveIconResolver / bootstrapDefaultIconResolver",
    "setPackEditorColorOverlay / setChartPaletteOverride",
    "window.chrome.setBackgroundRootColor",
    "emitCrossWindow",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemePackSelection {
    Wapp { wapp_id: String, theme_id: String },
    None,
    Unknown(String),
}

pub fn classify_theme_pack(pack_id: Option<&str>) -> ThemePackSelection {
    match pack_id {
        None => ThemePackSelection::None,
        Some(pack_id) => match parse_wapp_theme_pack_id(Some(pack_id)) {
            Some((wapp_id, theme_id)) => ThemePackSelection::Wapp { wapp_id, theme_id },
            None => ThemePackSelection::Unknown(pack_id.to_owned()),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemePackApplyResult {
    Ok,
    Err(String),
}

pub fn unknown_theme_pack_error(pack_id: &str) -> String {
    format!("unknown theme pack: {pack_id}")
}

pub fn invalid_wapp_theme_id_error(pack_id: &str) -> String {
    format!("invalid wapp theme id: {pack_id}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePackApplyPlan {
    pub selection: ThemePackSelection,
    pub reset: bool,
    pub sync_background: bool,
    pub notify: bool,
    pub broadcast_payload: Option<Option<String>>,
    pub result: ThemePackApplyResult,
}

pub fn plan_theme_pack_apply(
    pack_id: Option<&str>,
    broadcast: bool,
    wapp_error: Option<String>,
) -> ThemePackApplyPlan {
    let selection = classify_theme_pack(pack_id);
    match selection.clone() {
        ThemePackSelection::Wapp { .. } => {
            let pack_id = pack_id.unwrap_or("");
            let (result, payload) = match wapp_error {
                None => (ThemePackApplyResult::Ok, Some(pack_id.to_owned())),
                Some(message) => (ThemePackApplyResult::Err(message), None),
            };
            ThemePackApplyPlan {
                selection,
                reset: true,
                sync_background: true,
                notify: true,
                broadcast_payload: if broadcast { Some(payload) } else { None },
                result,
            }
        }
        ThemePackSelection::None => ThemePackApplyPlan {
            selection,
            reset: true,
            sync_background: true,
            notify: true,
            broadcast_payload: if broadcast { Some(None) } else { None },
            result: ThemePackApplyResult::Ok,
        },
        ThemePackSelection::Unknown(unknown) => ThemePackApplyPlan {
            selection,
            reset: true,
            sync_background: false,
            notify: false,
            broadcast_payload: None,
            result: ThemePackApplyResult::Err(unknown_theme_pack_error(&unknown)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_surface_is_pinned() {
        assert_eq!(THEME_PACK_APPLY_GAP.len(), 8);
        assert!(THEME_PACK_APPLY_GAP.contains(&"emitCrossWindow"));
        assert_eq!(crate::THEME_PACK_APPLY_GAP.len(), 8);
        assert_eq!(THEME_PACK_CHANGED_EVENT, "datazen:theme-pack-changed");
    }

    #[test]
    fn pack_ids_classify_into_wapp_none_and_unknown() {
        assert_eq!(classify_theme_pack(None), ThemePackSelection::None);
        assert_eq!(
            classify_theme_pack(Some("wapp:pub.name:dark")),
            ThemePackSelection::Wapp {
                wapp_id: "pub.name".to_owned(),
                theme_id: "dark".to_owned()
            }
        );
        assert_eq!(
            classify_theme_pack(Some("native-dark")),
            ThemePackSelection::Unknown("native-dark".to_owned())
        );
        assert_eq!(
            classify_theme_pack(Some("wapp::dark")),
            ThemePackSelection::Unknown("wapp::dark".to_owned())
        );
    }

    #[test]
    fn error_literals_are_pinned() {
        assert_eq!(unknown_theme_pack_error("x"), "unknown theme pack: x");
        assert_eq!(invalid_wapp_theme_id_error("x"), "invalid wapp theme id: x");
    }

    #[test]
    fn clearing_the_pack_resets_syncs_notifies_and_broadcasts_null() {
        let plan = plan_theme_pack_apply(None, true, None);
        assert_eq!(plan.selection, ThemePackSelection::None);
        assert!(plan.reset);
        assert!(plan.sync_background);
        assert!(plan.notify);
        assert_eq!(plan.broadcast_payload, Some(None));
        assert_eq!(plan.result, ThemePackApplyResult::Ok);
    }

    #[test]
    fn unknown_packs_reset_only_and_report_an_error() {
        let plan = plan_theme_pack_apply(Some("native"), true, None);
        assert!(plan.reset);
        assert!(!plan.sync_background);
        assert!(!plan.notify);
        assert_eq!(plan.broadcast_payload, None);
        assert_eq!(
            plan.result,
            ThemePackApplyResult::Err("unknown theme pack: native".to_owned())
        );
    }

    #[test]
    fn wapp_success_broadcasts_the_id_and_failure_broadcasts_null() {
        let success = plan_theme_pack_apply(Some("wapp:pub:dark"), true, None);
        assert!(success.reset);
        assert!(success.sync_background);
        assert_eq!(
            success.broadcast_payload,
            Some(Some("wapp:pub:dark".to_owned()))
        );
        assert_eq!(success.result, ThemePackApplyResult::Ok);

        let failure = plan_theme_pack_apply(
            Some("wapp:pub:dark"),
            true,
            Some("tokens.css missing".to_owned()),
        );
        assert!(failure.reset);
        assert_eq!(failure.broadcast_payload, Some(None));
        assert_eq!(
            failure.result,
            ThemePackApplyResult::Err("tokens.css missing".to_owned())
        );
    }

    #[test]
    fn broadcast_can_be_suppressed() {
        let plan = plan_theme_pack_apply(None, false, None);
        assert_eq!(plan.broadcast_payload, None);
        let plan = plan_theme_pack_apply(Some("wapp:p:t"), false, None);
        assert_eq!(plan.broadcast_payload, None);
    }
}
