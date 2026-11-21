use std::cell::RefCell;

use super::*;
use crate::{FkTextKind, GridDensity};

struct Translator {
    calls: RefCell<Vec<(String, String)>>,
}

impl FkSelectorTranslator for Translator {
    fn translate(&self, key: &str, target: &str) -> String {
        self.calls
            .borrow_mut()
            .push((key.to_owned(), target.to_owned()));
        format!("Pick {target} from {key}")
    }
}

#[derive(Default)]
struct ClipboardHost {
    copy_text: Option<String>,
    can_copy: bool,
    can_paste: bool,
    copied: RefCell<Vec<(String, bool)>>,
    pasted: RefCell<Vec<String>>,
    async_pastes: RefCell<Vec<String>>,
}

impl GridClipboardHandlers for ClipboardHost {
    fn build_copy_text(&self, _target: &GridCopyTarget) -> Option<String> {
        self.copy_text.clone()
    }

    fn on_copied(&self, target: &GridCopyTarget, ok: bool) {
        self.copied
            .borrow_mut()
            .push((target.mode().to_owned(), ok));
    }

    fn on_paste_text(&self, text: &str, _anchor: &ClipboardCellCoord) -> bool {
        self.pasted.borrow_mut().push(text.to_owned());
        true
    }

    fn on_paste_from_clipboard(&self, anchor: &ClipboardCellCoord) -> bool {
        self.async_pastes
            .borrow_mut()
            .push(anchor.column_name.clone());
        true
    }

    fn can_copy(&self) -> bool {
        self.can_copy
    }

    fn can_paste(&self) -> bool {
        self.can_paste
    }
}

#[test]
fn selector_label_uses_the_fk_selector_key_and_target() {
    let translator = Translator {
        calls: RefCell::new(Vec::new()),
    };
    let label = fk_selector_label(&translator, "users");
    assert_eq!(label, "Pick users from fkSelector.pickFrom");
    assert_eq!(FK_SELECTOR_PICK_FROM_KEY, "fkSelector.pickFrom");
    assert_eq!(
        translator.calls.borrow().as_slice(),
        &[("fkSelector.pickFrom".to_owned(), "users".to_owned())]
    );
}

#[test]
fn fk_text_closure_routes_only_the_selector_kind_to_the_translator() {
    let translator = Translator {
        calls: RefCell::new(Vec::new()),
    };
    let fallback =
        |kind: FkTextKind, target: &str| -> String { format!("fallback:{kind:?}:{target}") };
    let text = fk_text_with_selector(&translator, &fallback);
    assert_eq!(
        text(FkTextKind::Selector, "users"),
        "Pick users from fkSelector.pickFrom"
    );
    assert!(!text(FkTextKind::Jump, "users").contains("fkSelector"));
    assert_eq!(translator.calls.borrow().len(), 1);
}

#[test]
fn copy_targets_keep_the_row_or_cell_mode() {
    let rows = GridCopyTarget::row_target(vec![1, 3]);
    assert_eq!(rows.mode(), "row");
    assert_eq!(
        rows,
        GridCopyTarget::Row {
            row_indices: vec![1, 3]
        }
    );
    let range = GridClipboardRange {
        top_row: 0,
        bottom_row: 2,
        left_column: "a".to_owned(),
        right_column: "b".to_owned(),
    };
    let cell = GridCopyTarget::cell_target(range.clone());
    assert_eq!(cell.mode(), "cell");
    assert_eq!(cell, GridCopyTarget::Cell { range });
}

#[test]
fn clipboard_bridge_reports_availability_and_delivers_gestures() {
    let host = ClipboardHost {
        copy_text: Some("x".to_owned()),
        can_copy: true,
        can_paste: false,
        ..ClipboardHost::default()
    };
    assert_eq!(bridge_availability(&host), (true, false));
    assert_eq!(
        host.build_copy_text(&GridCopyTarget::row_target(vec![0]))
            .as_deref(),
        Some("x")
    );
    host.on_copied(&GridCopyTarget::row_target(vec![0]), true);
    assert_eq!(host.copied.borrow().as_slice(), &[("row".to_owned(), true)]);

    let anchor = ClipboardCellCoord {
        row_index: 2,
        column_name: "name".to_owned(),
    };
    assert!(host.on_paste_text("v", &anchor));
    assert!(host.on_paste_from_clipboard(&anchor));
    assert_eq!(host.pasted.borrow().as_slice(), &["v".to_owned()]);
    assert_eq!(host.async_pastes.borrow().as_slice(), &["name".to_owned()]);
}

#[test]
fn row_height_is_a_default_shifted_by_the_density_delta() {
    assert_eq!(DATA_TABLE_ROW_HEIGHT, 28);
    assert_eq!(density_row_height_delta(GridDensity::Compact), -6);
    assert_eq!(density_row_height_delta(GridDensity::Standard), 0);
    assert_eq!(density_row_height_delta(GridDensity::Relaxed), 8);
    assert_eq!(row_height_for_density(28, GridDensity::Compact), 22);
    assert_eq!(row_height_for_density(32, GridDensity::Standard), 32);
    assert_eq!(row_height_for_density(28, GridDensity::Relaxed), 36);
    assert_eq!(row_height_for_density(2, GridDensity::Compact), -4);
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(DATA_TABLE_BITS_SURFACE.len(), 10);
    assert!(DATA_TABLE_BITS_SURFACE.contains(&"rowHeightForDensity"));
    assert_eq!(DATA_TABLE_BITS_GAP.len(), 6);
    assert_eq!(crate::DATA_TABLE_BITS_SURFACE.len(), 10);
    assert_eq!(crate::DATA_TABLE_BITS_GAP.len(), 6);
    let _: Option<crate::GridDensity> = Some(GridDensity::Standard);
}
