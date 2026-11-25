//! Port of `windows/connection/TableFkErrorBanner.tsx` (DB-07).
//!
//! A refused foreign-key jump opens no tab; the originating table shows why.
//! The inline banner carries a classified conclusion (the translated reason)
//! above the host's original raw message, or just the raw message when the
//! reason is unknown/unclassified. Dismissing is an injected host action — no
//! fake IPC. `CopyableError` supplies the copy control the report needs.
//!
//! ## Keys
//!
//! `table-fk-error` is React's `testId`; `table-fk-error-text` is the copyable
//! message and `table-fk-error-close` is the dismiss control (a Rune
//! observation handle: React's close button has no test id).
//!
//! ## Local stand-ins (removal stage)
//!
//! No production host mounts this banner yet; the table host owns the error
//! slice. Every reusable item carries a per-item `#[allow(dead_code)]` with
//! removal stage "drop when the table grid host wires this banner".

use std::rc::Rc;

use rune::{row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

pub const ROOT_KEY: &str = "table-fk-error";
pub const TEXT_KEY: &str = "table-fk-error-text";
pub const CLOSE_KEY: &str = "table-fk-error-close";

/// React `{ code, raw }` of the refused jump.
#[allow(dead_code)] // VM mirror; remove when the host owns the error slice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkErrorVm {
    /// `GridErrorCode | 'unknown'`, e.g. `grid.fk.unknownConstraint`.
    pub code: String,
    pub raw: String,
}

/// The banner's props: `null` error means no banner (React returns `null`).
#[allow(dead_code)] // VM mirror; remove when the host wires the banner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableFkErrorBannerVm {
    pub error: Option<FkErrorVm>,
}

#[allow(dead_code)] // pure helper; remove with the host.
impl TableFkErrorBannerVm {
    /// React's `useMemo` message: conclusion + raw, or raw alone.
    pub fn message(&self) -> Option<String> {
        let error = self.error.as_ref()?;
        if error.code == "unknown" {
            return Some(error.raw.clone());
        }
        let reason = error.code.strip_prefix("grid.fk.").unwrap_or("");
        let title = if reason.is_empty() {
            String::new()
        } else {
            t(&format!("fkNav.error.{reason}")).to_owned()
        };
        if title.is_empty() {
            Some(error.raw.clone())
        } else {
            Some(format!("{title}\n{}", error.raw))
        }
    }
}

/// The sole React callback (`onDismiss`).
#[allow(dead_code)] // host port; remove when the host wires the banner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FkBannerAction {
    Dismiss,
}

#[allow(dead_code)] // host port; remove when the host wires the banner.
pub type FkBannerActionFn<S> = Rc<dyn Fn(&mut S, FkBannerAction) + 'static>;

/// Build the banner into `layer`; a null error mounts nothing.
#[allow(dead_code)] // view entry; remove when the host wires the banner.
pub fn build<S: 'static>(
    vm: &TableFkErrorBannerVm,
    on_action: &Option<FkBannerActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    // React returns null for a falsy composed message; only "" is falsy
    // (whitespace-only stays truthy), so do not trim.
    let Some(message) = vm.message().filter(|message| !message.is_empty()) else {
        return;
    };
    let k: &Kit = kit(dark);
    let mut close = text(t("common.close"))
        .key(CLOSE_KEY)
        .role("button")
        .focusable(true)
        .label(t("common.close"))
        .font(k.ty.text_small)
        .color(k.c.text_accent)
        .help(t("common.close"));
    if let Some(callback) = on_action.clone() {
        close = close.on_press(move |s: &mut S| callback(s, FkBannerAction::Dismiss));
    }
    layer.push(
        row()
            .key(ROOT_KEY)
            .role("alert")
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .center_y()
            .px(12.0)
            .py(6.0)
            .fill(k.alpha(k.c.danger.fg, 0.10))
            .child(
                CopyableError::new(TEXT_KEY, message)
                    .copy_button(true)
                    .build::<S>(k)
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(close.shrink()),
    );
}
