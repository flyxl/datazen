//! Port of `packages/frontend/src/windows/onboarding/BrandSidebar.tsx`.
//!
//! The onboarding wizard's brand sidebar: a fixed-dark gradient column with
//! the DataZen mark, heading and description, three feature highlights and
//! the app-version footer.
//!
//! ## What maps where
//!
//! - `getPlatformOrNull()?.hostSettings?.getAppVersion()` is a host concern:
//!   the version arrives as a plain prop ([`BrandSidebar::version`], the
//!   React `useState('…')` default included). The `build-constant`
//!   `versionSource` early-return and the `.catch(() => {})` both collapse
//!   to "the host simply never delivers" — the em-dash placeholder stays,
//!   exactly as React leaves it. [`version_label`] is the `v{version}`
//!   format and this file's only logic.
//! - The layout metrics (`WIZARD_ASIDE_PX_CLASS` 34px, `WIZARD_FOOTER_HEIGHT_CLASS`
//!   56px, `WIZARD_CHROME_TEXT_CLASS` 11.5px mono muted) come from
//!   `windows/onboarding/onboardingLayout.ts`, which is this segment's own
//!   later task (T287); until it lands they live here as
//!   [`ASIDE_PAD_X`] / [`FOOTER_HEIGHT`] / the chrome text style, the seam
//!   T287 absorbs.
//! - The gradient background is React's one always-dark brand anchor: the
//!   linear pair and the radial tint are fixed colours from `ui/palette.rs`
//!   (`brand_sidebar_gradient` / `brand_sidebar_radial_tint`), painted on
//!   the root el regardless of `dark` (Rune's radial draws after the
//!   gradient — the same stacking as React's two background layers — and
//!   both clip to the el's own bounds, which is what React's
//!   `overflow-hidden` and the absolute overlay div were for). React's
//!   text classes stay theme-relative — in a light-themed app the sidebar
//!   text really is dark-on-dark there, and this port mirrors that (a
//!   known React behaviour, not silently "fixed"). The CSS percentage
//!   radius (`120% 70%`) becomes points at the default onboarding window
//!   (960×720: rx 316.8, ry 378) — a documented delta, since Rune radii
//!   are points, not box-relative.
//! - `text-fg-secondary` / `text-fg-muted` both map to the kit's one
//!   `text_muted` token (the established mapping of the T015/T029 ports;
//!   the kit's `text_faint` is reserved for placeholder ink).
//! - Icons: the registry's `ICON_CONNECTIONS` (lucide "database"),
//!   `ICON_DOWNLOAD`, `ICON_SPARKLES` and `ICON_DASHBOARDS` (lucide
//!   "gauge") — no new registry entries.
//! - `pointer-events-none` on the gradient overlay needs no counterpart: an
//!   el without handlers never intercepts. `first:border-t-0 first:pt-0`
//!   becomes the per-item builder flag.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the rest derive from the root:
//!
//! | part | key |
//! |------|-----|
//! | root aside | `onboarding-sidebar` |
//! | brand mark / wordmark | `onboarding-sidebar-brand`, `onboarding-sidebar-brand-name` |
//! | heading / description | `onboarding-sidebar-heading`, `onboarding-sidebar-desc` |
//! | feature row / icon / title / desc | `onboarding-sidebar-feature-{i}`, `-icon`, `-title`, `-desc` |
//! | version footer / label | `onboarding-sidebar-foot`, `onboarding-version-label` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `max-w-[32ch]` on the description is not reproduced (the
//! sidebar column width already binds first for zh-CN); `tracking-tight`
//! maps to −0.025em in points; the radial radius is nominal for the
//! default window.

use rune::{col, icon, row, text, Color, El, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{
    ICON_CONNECTIONS, ICON_DASHBOARDS, ICON_DOWNLOAD, ICON_SPARKLES,
};
use crate::ui::kit::kit;
use crate::ui::palette;

/// React `WIZARD_ASIDE_PX_CLASS` (`px-[34px]`), the sidebar's horizontal
/// rhythm (34px → 25.5pt). The seam `onboardingLayout.ts` (T287) absorbs.
#[allow(dead_code)] // reusable port; wired by the onboarding wizard track
pub(crate) const ASIDE_PAD_X: f32 = 25.5;
/// React `WIZARD_FOOTER_HEIGHT_CLASS` (`h-[56px]`, 56px → 42pt): the version
/// row and the wizard's step indicator share this height so both footers
/// end on the same bottom line.
#[allow(dead_code)] // reusable port; wired by the onboarding wizard track
pub(crate) const FOOTER_HEIGHT: f32 = 42.0;
/// The sidebar's fixed width (`w-[352px]`, 352px → 264pt).
#[allow(dead_code)] // reusable port; wired by the onboarding wizard track
pub(crate) const ASIDE_WIDTH: f32 = 264.0;

/// React `v{version}` — the version footer's label text, this file's only
/// logic (the `useState('…')` default is the caller's).
#[allow(dead_code)] // reusable port; wired by the onboarding wizard track
pub(crate) fn version_label(version: &str) -> String {
    format!("v{version}")
}

/// React `WIZARD_CHROME_TEXT_CLASS` (`text-[11.5px] font-mono text-fg-muted`).
fn chrome_text() -> TextStyle {
    TextStyle::new(8.625, 400.0).mono()
}

/// The brand sidebar. Purely presentational; the version is host-delivered.
#[allow(dead_code)] // reusable port; wired by the onboarding wizard track
pub(crate) struct BrandSidebar {
    version: String,
}

impl Default for BrandSidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl BrandSidebar {
    /// React's `useState('…')` start.
    #[allow(dead_code)] // reusable port; wired by the onboarding wizard track
    pub(crate) fn new() -> Self {
        Self { version: "…".to_owned() }
    }

    /// The host-delivered app version (`hostSettings.getAppVersion()`).
    #[allow(dead_code)] // reusable port; wired by the onboarding wizard track
    pub(crate) fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Paint the sidebar; `dark` drives the text tokens only — the
    /// gradient anchor stays fixed-dark, as in React.
    #[allow(dead_code)] // reusable port; wired by the onboarding wizard track
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let (top, bottom) = palette::brand_sidebar_gradient();

        let features = [
            (&ICON_DOWNLOAD, "onboarding.sidebar.importTitle", "onboarding.sidebar.importDesc"),
            (&ICON_SPARKLES, "onboarding.sidebar.aiTitle", "onboarding.sidebar.aiDesc"),
            (&ICON_DASHBOARDS, "onboarding.sidebar.dashboardTitle", "onboarding.sidebar.dashboardDesc"),
        ];

        col()
            .key("onboarding-sidebar")
            .w(ASIDE_WIDTH)
            .shrink() // React shrink-0
            .h_full()
            // The always-dark brand anchor: the linear pair plus the radial
            // accent tint (React paints the radial over the linear — Rune's
            // radial draws after the gradient, same stacking; both clip to
            // this el's own bounds, which is what React's overflow-hidden
            // was for). pointer-events-none needs no counterpart: an el
            // without handlers never intercepts.
            .gradient(top, bottom)
            .radial_gradient(
                [0.0, 0.0],
                [316.8, 378.0], // 120% / 70% of 352px / 720px (the default window)
                &[(0.0, palette::brand_sidebar_radial_tint()), (0.55, Color::TRANSPARENT)],
            )
            .border_sides([0.0, 0.75, 0.0, 0.0], k.c.border_base) // border-r border-edge
            .px(ASIDE_PAD_X) // WIZARD_ASIDE_PX_CLASS
            .pt(30.0) // React pt-[40px]
            // Brand row (React mb-[38px], gap-2.5).
            .child(
                row()
                    .key("onboarding-sidebar-brand")
                    .gap(7.5)
                    .center_y()
                    .mb(28.5)
                    .child(icon(&ICON_CONNECTIONS, 15.75).color(k.c.text_accent))
                    .child(
                        text("DataZen".to_owned())
                            .key("onboarding-sidebar-brand-name")
                            .font(TextStyle::new(12.0, 700.0).tracking(-0.3)) // text-base font-bold tracking-tight
                            .color(k.c.text_base),
                    ),
            )
            // Heading (React text-[26px] font-bold leading-tight, mb-3).
            .child(
                text(t("onboarding.sidebar.heading").to_owned())
                    .key("onboarding-sidebar-heading")
                    .font(TextStyle::new(19.5, 700.0).tracking(-0.4875))
                    .mb(9.0)
                    .color(k.c.text_base),
            )
            // Description (React text-[13.5px] leading-[1.65], mb-[34px]).
            .child(
                text(t("onboarding.sidebar.desc").to_owned())
                    .key("onboarding-sidebar-desc")
                    .font(TextStyle::new(10.125, 400.0).lh(16.7))
                    .mb(25.5)
                    .color(k.c.text_muted),
            )
            // Feature highlights (React's three FeatureItem rows).
            .child(feature_list(&k, &features))
            // React mt-auto: a flexible spacer absorbs the free space, so the
            // version footer shares the wizard footer's height and both end
            // on the same bottom line.
            .child(rune::el().key("onboarding-sidebar-spacer").grow(1.0))
            .child(
                row()
                    .key("onboarding-sidebar-foot")
                    .h(FOOTER_HEIGHT)
                    .shrink()
                    .center_y()
                    .child(
                        text(version_label(&self.version))
                            .key("onboarding-version-label")
                            .font(chrome_text())
                            .color(k.c.text_muted),
                    ),
            )
    }
}

/// The three feature rows, in React's order; the first row has no top
/// border and no top padding (`first:border-t-0 first:pt-0`).
fn feature_list(k: &Kit, features: &[(&'static rune::Icon, &'static str, &'static str)]) -> El {
    let mut list = col().key("onboarding-sidebar-features").w_full();
    for (index, (icon_id, title_key, desc_key)) in features.iter().enumerate() {
        list = list.child(feature_item(k, index, icon_id, title_key, desc_key));
    }
    list
}

/// One `FeatureItem`: icon well (30px, accent-tinted, bordered) + title +
/// description; separated from the row above by `border-t border-edge/70`.
fn feature_item(
    k: &Kit,
    index: usize,
    icon_id: &'static rune::Icon,
    title_key: &str,
    desc_key: &str,
) -> El {
    let first = index == 0;
    let mut item = row()
        .key(format!("onboarding-sidebar-feature-{index}"))
        .gap(9.0) // React gap-3
        .center_y()
        .py(9.0) // React py-3
        .border_sides(
            [if first { 0.0 } else { 0.75 }, 0.0, 0.0, 0.0],
            k.c.border_base.alpha(0.7), // border-edge/70
        );
    if first {
        item = item.pt(0.0); // first:pt-0
    }
    item.child(
        // The 30px icon well (rounded-lg, bg-accent/12, border-accent/16).
        rune::el()
            .key(format!("onboarding-sidebar-feature-{index}-icon"))
            .size(22.5, 22.5)
            .shrink()
            .center()
            .radius(6.0)
            .fill(k.c.bg_accent.alpha(0.12))
            .border(0.75, k.c.bg_accent.alpha(0.16)) // border-accent/16
            .child(icon(icon_id, 11.25).color(k.c.text_accent)),
    )
    .child(
        col()
            .key(format!("onboarding-sidebar-feature-{index}-text"))
            .child(
                text(t(title_key).to_owned())
                    .key(format!("onboarding-sidebar-feature-{index}-title"))
                    .font(TextStyle::new(9.75, 600.0).tracking(-0.24)) // 13px semibold tracking-tight
                    .color(k.c.text_base),
            )
            .child(
                text(t(desc_key).to_owned())
                    .key(format!("onboarding-sidebar-feature-{index}-desc"))
                    .font(TextStyle::new(9.0, 400.0).lh(11.25)) // text-xs leading-relaxed
                    .mt(0.75) // React mt-px
                    .color(k.c.text_muted),
            ),
    )
}
