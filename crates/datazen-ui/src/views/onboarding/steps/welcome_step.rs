//! Port of `packages/frontend/src/windows/onboarding/steps/WelcomeStep.tsx`
//! (T247): S0 — the journey's welcome screen, three stacked entry cards
//! (import / manual / sample) under the heading and subtitle.
//!
//! The React file also exports `StepTag` for the other steps; that export
//! was hoisted into its own module by T148 ([`super::step_tag`]) because
//! `AiProviderStep.tsx` imports it from here — this port reuses it and does
//! NOT render it (React's `WelcomeStep` renders no tag of its own).
//!
//! ## What maps where
//!
//! - The component is stateless: `detectedApp` (the first client config
//!   `useDetectedImportSource` found on this machine — a T306 host concern)
//!   is [`WelcomeStep::detected_app`], `selectedEntry` (the T323 machine's
//!   `state.entry`) is [`WelcomeStep::selected_entry`], and the three card
//!   actions are `Rc` action slots ([`WelcomeStep::on_import`] /
//!   [`WelcomeStep::on_manual`] / [`WelcomeStep::on_sample`] — the host's
//!   `ENTER_IMPORT` / `ENTER_MANUAL` / `ENTER_SAMPLE` dispatches). No
//!   effects, no timers.
//! - `CONNECTION_IMPORT_APP_LABEL` (`ConnectionShareDialog.tsx:17` — the
//!   five client display names) is [`import_app_label`] here; the
//!   ConnectionShareDialog port (the import track) absorbs it with the
//!   [`ImportApp`] seam type (the T021→T287 absorb pattern).
//! - The selected-entry highlight is the import card's alone in React (its
//!   is the only class list that branches on `selectedEntry`): selected →
//!   `border-accent/55` + the 3px accent ring at 10%, otherwise
//!   `border-accent/45` + the ring at 6%. The manual and sample cards wear
//!   the plain `border-edge` chrome whatever the entry.
//! - i18n: the seven `t("onboarding.s0.*")` reads port to `crate::i18n::t`
//!   with the same keys; `importDetectedApp`'s `{app}` placeholder is
//!   formatted with the label, exactly as React's `t(…, { app })` does.
//! - Icons: the registry's `ICON_DOWNLOAD`, `ICON_PLUS` and
//!   `ICON_FLASK_CONICAL` in the wells, `ICON_ARROW_RIGHT` on every card's
//!   trailing edge (the two new T247 registry entries).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the header derives under the
//! root's `onboarding-step-s0-` prefix and each card's parts under its own
//! testid:
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s0"`) | `onboarding-step-s0` |
//! | heading `h2` | `onboarding-step-s0-title` |
//! | subtitle `p` | `onboarding-step-s0-subtitle` |
//! | the cards column (`flex flex-col gap-3`) | `onboarding-step-s0-cards` |
//! | import card (`data-testid="onboarding-entry-import"`) | `onboarding-entry-import` |
//! | import card's icon well / text col / title / desc / badge / arrow | `onboarding-entry-import-well` / `-text` / `-title` / `-desc` / `-badge` / `-arrow` |
//! | detected-app line (`data-testid="onboarding-detected-app"`) | `onboarding-detected-app` |
//! | manual card (`data-testid="onboarding-entry-manual"`) + parts | `onboarding-entry-manual` (`-well` / `-text` / `-title` / `-desc` / `-arrow`) |
//! | sample card (`data-testid="onboarding-entry-sample"`) + parts | `onboarding-entry-sample` (`-well` / `-text` / `-title` / `-desc` / `-arrow`) |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `max-w-[56ch]` on the subtitle becomes 283.5pt (the `ch` unit
//! approximation, the `ai_provider_step.rs` ruling); the cards'
//! `hover:-translate-y-px` lift, the arrows' `group-hover` ink/shift and
//! every `hover:border-*` recolour have no Rune counterpart (`El::hover` is
//! a fill hover) — the import card hovers the `accent-dim`/`#141a26` fill
//! and the plain cards `surface-raised`, the borders keep their rest ink;
//! `transition-all` is static; the dark-mode card gradient
//! (`#121824→#0f131c`) and its `#141a26` hover live in `ui/palette.rs`
//! (`welcome_card_gradient` / `welcome_card_hover`), the fixed-colour
//! ruling; `text-green` on the detected line is the kit's `success.fg`
//! (the `execution_result_panel.rs` mapping); `text-on-accent` on the
//! FASTEST pill is `palette::on_accent`.

// The onboarding wizard host (T114's `welcome` seat) is the production
// consumer; until the window track wires it, this crate's audit is the
// only caller.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, Shadow, TextStyle};

use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{ICON_ARROW_RIGHT, ICON_DOWNLOAD, ICON_FLASK_CONICAL, ICON_PLUS};
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::views::onboarding::onboarding_wizard::{Entry, ImportApp};

/// The step's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// React `max-w-[640px]` (640px → 480pt): the step column's cap.
pub const STEP_MAX_W: f32 = 480.0;

/// React `max-w-[56ch]` on the subtitle: 56 × the "0" advance (≈ 0.5em at
/// 13.5px → 6.75px) ≈ 378px → 283.5pt. Approximate by construction — the
/// `ch` unit has no Rune counterpart (the `ai_provider_step.rs` ruling).
const SUBTITLE_MAX_W: f32 = 283.5;

/// React `CONNECTION_IMPORT_APP_LABEL`
/// (`components/connection/ConnectionShareDialog.tsx:17`): the five
/// clients' display names. The ConnectionShareDialog port (the import
/// track, T177/T306) absorbs this map with the `ImportApp` seam type.
pub fn import_app_label(app: ImportApp) -> &'static str {
    match app {
        ImportApp::Dbx => "DBX",
        ImportApp::Navicat => "Navicat",
        ImportApp::Datagrip => "DataGrip",
        ImportApp::Dbeaver => "DBeaver",
        ImportApp::Tableplus => "TablePlus",
    }
}

/// The journey's welcome screen: the heading, the subtitle and the three
/// stacked entry cards. Stateless — every prop arrives per frame.
pub struct WelcomeStep<S: 'static> {
    /// `detectedApp` — the first client config found on this machine
    /// (`None`: nothing detected, or detection not finished yet).
    detected_app: Option<ImportApp>,
    /// `selectedEntry` — the entry the wizard's state has picked (the
    /// import card's highlight branch; the cards stay clickable).
    selected_entry: Entry,
    /// `onImport`.
    on_import: Option<ActionFn<S>>,
    /// `onManual`.
    on_manual: Option<ActionFn<S>>,
    /// `onSample`.
    on_sample: Option<ActionFn<S>>,
}

impl<S: 'static> WelcomeStep<S> {
    /// React's mount on S0: `selectedEntry` is the machine's `state.entry`
    /// (the fresh journey's `manual`), nothing detected yet, no actions
    /// wired until the host hands its dispatches over.
    pub fn new(selected_entry: Entry) -> Self {
        Self {
            detected_app: None,
            selected_entry,
            on_import: None,
            on_manual: None,
            on_sample: None,
        }
    }

    /// `detectedApp` — the detected client whose config seeds the import
    /// card's sub-line.
    pub fn detected_app(mut self, app: ImportApp) -> Self {
        self.detected_app = Some(app);
        self
    }

    /// `selectedEntry`.
    pub fn selected_entry(mut self, entry: Entry) -> Self {
        self.selected_entry = entry;
        self
    }

    /// `onImport` → the host's `ENTER_IMPORT` dispatch.
    pub fn on_import(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_import = Some(Rc::new(f));
        self
    }

    /// `onManual` → the host's `ENTER_MANUAL` dispatch.
    pub fn on_manual(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_manual = Some(Rc::new(f));
        self
    }

    /// `onSample` → the host's `ENTER_SAMPLE` dispatch.
    pub fn on_sample(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_sample = Some(Rc::new(f));
        self
    }

    /// Paint the step. `dark` drives the kit tokens (and the cards'
    /// fixed-dark gradient pair, which the light theme replaces with the
    /// flat surface fill).
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);

        col()
            .key("onboarding-step-s0") // React data-testid, verbatim
            .w_full()
            .max_w(STEP_MAX_W) // max-w-[640px]
            // The heading: mb-2.5 text-[25px] font-bold tracking-tight
            // leading-tight text-fg (the AiProviderStep heading's metrics).
            .child(
                text(t("onboarding.s0.title").to_owned())
                    .key("onboarding-step-s0-title")
                    .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                    .mb(7.5)
                    .color(k.c.text_base),
            )
            // The subtitle: mb-7 max-w-[56ch] text-[13.5px] leading-[1.65]
            // text-fg-secondary.
            .child(
                text(t("onboarding.s0.subtitle").to_owned())
                    .key("onboarding-step-s0-subtitle")
                    .font(TextStyle::new(10.125, 400.0).lh(16.7))
                    .max_w(SUBTITLE_MAX_W)
                    .mb(21.0)
                    .color(k.c.text_muted),
            )
            // The three cards: flex flex-col gap-3.
            .child(
                col()
                    .key("onboarding-step-s0-cards")
                    .w_full()
                    .gap(12.0)
                    // Import — the primary card (accent chrome, the FASTEST
                    // pill, the detected-client sub-line).
                    .child(entry_card(
                        &k,
                        dark,
                        CardChrome::Import { selected: self.selected_entry == Entry::Import },
                        "onboarding-entry-import",
                        &ICON_DOWNLOAD,
                        "onboarding.s0.importCard",
                        "onboarding.s0.importCardDesc",
                        // The FASTEST pill — import only.
                        Some(fastest_pill(&k, dark)),
                        self.detected_app,
                        self.on_import,
                    ))
                    // Manual — the connection form entry.
                    .child(entry_card(
                        &k,
                        dark,
                        CardChrome::Plain,
                        "onboarding-entry-manual",
                        &ICON_PLUS,
                        "onboarding.s0.manualCard",
                        "onboarding.s0.manualCardDesc",
                        None,
                        None,
                        self.on_manual,
                    ))
                    // Sample — the bundled playground entry.
                    .child(entry_card(
                        &k,
                        dark,
                        CardChrome::Plain,
                        "onboarding-entry-sample",
                        &ICON_FLASK_CONICAL,
                        "onboarding.s0.sampleCard",
                        "onboarding.s0.sampleCardDesc",
                        None,
                        None,
                        self.on_sample,
                    )),
            )
    }
}

/// Which border/ring chrome a card wears: the import card's accent chrome
/// (sharpening while it is the selected entry) or the plain edge chrome of
/// the manual and sample cards.
enum CardChrome {
    /// `border-accent/55 shadow-[…0.10]` when selected, else
    /// `border-accent/45 shadow-[…0.06]`; hover is the accent dim.
    Import { selected: bool },
    /// `border-edge`, no ring; hover is the raised surface.
    Plain,
}

/// One entry card: the shared `group flex items-center gap-4 rounded-xl
/// border p-[18px_20px] text-left` row with the accent icon well, the
/// title/description column, the optional FASTEST pill, the optional
/// detected-client line and the trailing arrow.
fn entry_card<S: 'static>(
    k: &Kit,
    dark: bool,
    chrome: CardChrome,
    testid: &'static str,
    icon_id: &'static rune::Icon,
    title_key: &str,
    desc_key: &str,
    badge: Option<El>,
    detected: Option<ImportApp>,
    on_press: Option<ActionFn<S>>,
) -> El {
    // The border, the 3px accent ring and the hover fill of this chrome.
    let (border_color, ring_alpha, hover_fill) = match chrome {
        CardChrome::Import { selected: true } => {
            (k.c.bg_accent.alpha(0.55), 0.10, palette::welcome_card_hover(dark))
        }
        CardChrome::Import { selected: false } => {
            (k.c.bg_accent.alpha(0.45), 0.06, palette::welcome_card_hover(dark))
        }
        CardChrome::Plain => (k.c.border_base, 0.0, k.c.surface_raised),
    };

    // The text column: min-w-0 flex-1.
    let mut text_col = col()
        .key(format!("{testid}-text"))
        .grow(1.0) // flex-1
        .min_w(0.0) // min-w-0
        .child(
            // <b>: block text-[14.5px] font-semibold tracking-tight text-fg.
            text(t(title_key).to_owned())
                .key(format!("{testid}-title"))
                .font(TextStyle::new(10.875, 600.0).tracking(-0.272))
                .color(k.c.text_base),
        )
        .child(
            // The description: mt-0.5 block text-[12.5px] leading-relaxed
            // text-fg-secondary.
            text(t(desc_key).to_owned())
                .key(format!("{testid}-desc"))
                .font(TextStyle::new(9.375, 400.0).lh(15.23))
                .mt(1.5)
                .color(k.c.text_muted),
        );
    // The detected-client line — the import card's third line, only when a
    // client config was found on this machine.
    if let Some(app) = detected {
        text_col = text_col.child(
            text(t("onboarding.s0.importDetectedApp").replace("{app}", import_app_label(app)))
                .key("onboarding-detected-app") // React data-testid, verbatim
                .font(TextStyle::new(8.625, 400.0).mono()) // font-mono text-[11.5px]
                .mt(1.5)
                .color(k.c.success.fg), // text-green
        );
    }

    // React: `<button type="button">` — a raw button.
    let mut card = row()
        .key(testid) // React data-testid, verbatim
        .w_full()
        .center_y() // items-center
        .gap(16.0) // gap-4
        .radius(k.radius.lg) // rounded-xl
        .border(0.75, border_color)
        .px(15.0) // p-[18px_20px] → the 20px sides
        .py(13.5) // …and the 18px top/bottom
        .hover(hover_fill)
        .role("button")
        .focusable(true)
        // The 40px icon well: rounded-[11px] bg-accent/12 text-accent
        // border-accent/18 around the 19px glyph.
        .child(
            rune::el()
                .key(format!("{testid}-well"))
                .size(30.0, 30.0) // h-10 w-10 shrink-0
                .center()
                .radius(8.25) // rounded-[11px]
                .fill(k.c.bg_accent.alpha(0.12)) // bg-accent/12
                .border(0.75, k.c.bg_accent.alpha(0.18)) // border-accent/18
                .child(icon(icon_id, 14.25).color(k.c.text_accent)), // h-[19px]
        )
        .child(text_col);
    // bg-surface, or the fixed-dark pair under `dark:` (palette-owned) —
    // the dark hover's flat fill then replaces the gradient, as in React.
    if dark {
        let (top, bottom) = palette::welcome_card_gradient();
        card = card.gradient(top, bottom);
    } else {
        card = card.fill(k.c.surface_base);
    }
    // The import card's accent ring — a 3px outer box shadow
    // (`shadow-[0_0_0_3px_rgba(79,195,247,…)]`, the accent at that alpha).
    if ring_alpha > 0.0 {
        card = card.box_shadow(Shadow {
            spread: 2.25, // 3px
            color: k.c.bg_accent.alpha(ring_alpha),
            inset: false,
            ..Shadow::default()
        });
    }
    // The FASTEST pill rides between the text column and the arrow
    // (import only).
    if let Some(badge) = badge {
        card = card.child(badge);
    }
    // The trailing arrow: h-4 w-4 shrink-0 text-fg-muted (the group-hover
    // accent/shift has no counterpart — see the deltas).
    card = card.child(
        icon(&ICON_ARROW_RIGHT, 12.0)
            .key(format!("{testid}-arrow"))
            .shrink()
            .color(k.c.text_muted),
    );
    if let Some(on_press) = on_press {
        card = card.on_press(move |s: &mut S| on_press(s));
    }
    card
}

/// The FASTEST pill: `shrink-0 rounded-full bg-accent px-2.5 py-[3px]
/// text-[10px] font-bold uppercase tracking-wider text-on-accent`.
fn fastest_pill(k: &Kit, dark: bool) -> El {
    row()
        .key("onboarding-entry-import-badge")
        .shrink() // shrink-0
        .radius(999.0) // rounded-full
        .fill(k.c.bg_accent) // bg-accent
        .px(7.5) // px-2.5
        .py(2.25) // py-[3px]
        .center_y()
        .child(
            // `uppercase` at render time (a no-op for the en value, the
            // column_mapping_editor ruling).
            text(t("onboarding.s0.importFastest").to_uppercase())
                .font(TextStyle::new(7.5, 700.0).tracking(0.375)) // text-[10px] tracking-wider
                .color(palette::on_accent(dark)), // text-on-accent
        )
}
