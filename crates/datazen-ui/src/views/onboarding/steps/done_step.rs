//! Port of `packages/frontend/src/windows/onboarding/steps/DoneStep.tsx`
//! (T177): S3 — the journey's done screen.
//!
//! The React file is a pure summary over the wizard state: the centered
//! green check well (its own inline SVG check, not a lucide glyph), the
//! heading and subtitle, the three-row summary card (connection / AI
//! provider / storage — the connection row's value following
//! `connectionSummary(state)`, the AI row's the configured label, the
//! storage row's the fixed local-encryption line), the sample-entry hint
//! (`💡` + `nextHint`, rich HTML with two `<b>` segments), and the primary
//! "open DataZen" button — the journey's only exit, whose `onOpen` is the
//! wizard's `handleOpen` (T114's `on_open` seam).
//!
//! ## What maps where
//!
//! - The component is prop-driven (`{ state, onOpen }`); the state halves
//!   it reads arrive as feed-ins, exactly the `WelcomeStep` convention:
//!   [`DoneStep::entry`] (`state.entry` — only the sample entry shows the
//!   hint), [`DoneStep::import_result`] (`state.importResult`),
//!   [`DoneStep::connection_name`] (`state.connectionName`),
//!   [`DoneStep::ai_label`] / [`DoneStep::ai_configured`] (`state.aiLabel`
//!   / `state.aiConfigured`). `state.step` and `state.samplePath` are
//!   never read here. `onOpen` is the action slot [`DoneStep::on_open`]
//!   (the host's handleOpen wiring — T114's `open_action()`).
//! - `connectionSummary` / `importedCount` (`wizardState.ts:149/186`) are
//!   absorbed into `wizard_state.rs` (T323, the state machine's port —
//!   the T021→T287 absorb pattern) and re-exported here as
//!   [`connection_summary`] / [`imported_count`]. [`ImportSummary`] comes
//!   from the import step's port (the producer's side of the same seam).
//! - The three summary values are React's nested ternaries, verbatim:
//!   `connValue` (imported → `importedValue`/`updatedValue` with
//!   `{count}`/`{source}`, connection → the name, else `notConfigured`),
//!   `aiValue` (`aiConfigured && aiLabel ? aiLabel : aiNotConfigured`)
//!   and the fixed `storageValue`. The `skipped` flags (the check glyph's
//!   muted ink) are `summary.kind === 'none'` and `!state.aiConfigured`;
//!   the storage row is never skipped and is the `last` one (no bottom
//!   border).
//! - The hint's `dangerouslySetInnerHTML`: `💡 ${t('…nextHint')}` — the
//!   zh-CN pack entry carries two `<b>…</b>` segments, so
//!   [`strip_b_tags`] removes every pair and the [`Span`]s bold the inner
//!   byte ranges (the manual step's one-pair ruling, generalized). A
//!   string with an unclosed pair degrades to plain text.
//! - i18n: the ten reads port to `crate::i18n::t` with the same keys
//!   (`onboarding.s3.title` … `openBtn`); the `{count}`/`{source}`
//!   placeholders are string-replaced, exactly as React's `t(…, { count,
//!   source })` does.
//! - Icons: the registry's `ICON_CIRCLE_CHECK` (lucide `circle-check-big`
//!   — React 0.487's `CheckCircle2`, the T107 ruling) on the summary
//!   rows, and `ICON_DONE_CHECK` — the new T177 registry entry carrying
//!   the React file's own inline check SVG (`m4 12.5 5 5L20 6.5` at
//!   stroke-width 2.4) verbatim.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; this step's derived keys sit
//! under the one `onboarding-step-s3-` prefix and the summary rows' parts
//! under React's own `onboarding-summary-` testid prefix (the manual
//! step's ruling):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s3"`) | `onboarding-step-s3` |
//! | the check well | `onboarding-step-s3-check` |
//! | the title block (`text-center` wrapper) | `onboarding-step-s3-heading` |
//! | heading `h2` | `onboarding-step-s3-title` |
//! | subtitle `p` | `onboarding-step-s3-subtitle` |
//! | the summary card | `onboarding-step-s3-summary` |
//! | connection row (`data-testid="onboarding-summary-connection"`) | `onboarding-summary-connection` |
//! | that row's check / label / value | `onboarding-summary-connection-check` / `-label` / `-value` |
//! | AI row (`data-testid="onboarding-summary-ai"`) + parts | `onboarding-summary-ai` (`-check` / `-label` / `-value`) |
//! | storage row (`data-testid="onboarding-summary-storage"`) + parts | `onboarding-summary-storage` (`-check` / `-label` / `-value`) |
//! | the sample hint | `onboarding-step-s3-hint` |
//! | open button (`data-testid="onboard-open-datazen"`) | `onboard-open-datazen` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `max-w-[48ch]` on the subtitle becomes 243pt and
//! `max-w-[480px]` on the card 360pt (the `ch` approximation, the
//! `ai_provider_step.rs` ruling); the well's
//! `shadow-[0_0_0_8px_rgba(95,208,138,0.04)]` is an 8px (6pt) spread
//! ring in the kit success green at that alpha (the fixed `rgba(95,
//! 208,138,…)` green IS the React `green` token, which every port maps
//! to `success.fg` — the `welcome_step.rs` ruling); `rounded-full` is the
//! 999pt radius (clamped to half the shorter side); `rounded-xl` is the
//! kit's `lg` token (the `workspace_default_cards.rs` mapping); the
//! value cells' `text-ellipsis` is `truncate`; `whitespace-nowrap` folds
//! into it.

// The onboarding wizard host (T114's `-step-s3` seat) is the production
// consumer; until the window track wires it, this crate's audit is the
// only caller.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, rich, row, text, Align, El, Shadow, Span, TextStyle};

use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_CIRCLE_CHECK, ICON_DONE_CHECK};
use crate::ui::kit::kit;
use crate::views::onboarding::onboarding_wizard::Entry;
// The T323 absorb: the done screen's summary cores live in `wizard_state.rs`
// (the state machine's own derivations); re-exported here so the audit's
// `done_step::{…}` import path stays untouched (`imported_count` is the
// audit's direct entry — the build itself reaches it through
// `connection_summary`).
#[allow(unused_imports)]
pub(crate) use crate::views::onboarding::wizard_state::{
    ConnectionSummary, connection_summary, imported_count,
};

use super::import_step::ImportSummary;

/// The step's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// React `max-w-[48ch]` on the subtitle: 48 × the "0" advance (≈ 0.5em
/// at 13.5px → 6.75px) ≈ 324px → 243pt. Approximate by construction —
/// the `ch` unit has no Rune counterpart (the `ai_provider_step.rs`
/// ruling).
const SUBTITLE_MAX_W: f32 = 243.0;

/// React `max-w-[480px]` (480px → 360pt): the summary card's cap.
const SUMMARY_MAX_W: f32 = 360.0;

/// The hint's `dangerouslySetInnerHTML` handling: strip every
/// `<b>…</b>` pair and report the inner byte ranges for the bold
/// [`Span`]s (the manual step's one-pair helper, generalized to the
/// two-pair `nextHint`). A string with an unclosed pair degrades to
/// plain text — future locales without complete tags keep their words.
fn strip_b_tags(raw: &str) -> (String, Vec<std::ops::Range<usize>>) {
    let mut plain = String::new();
    let mut bold = Vec::new();
    let mut rest = raw;
    while let Some(open) = rest.find("<b>") {
        let after = &rest[open + 3..];
        let Some(close) = after.find("</b>") else {
            plain.push_str(rest);
            return (plain, bold);
        };
        let start = plain.len() + open;
        plain.push_str(&rest[..open]);
        plain.push_str(&after[..close]);
        bold.push(start..start + close);
        rest = &after[close + 4..];
    }
    plain.push_str(rest);
    (plain, bold)
}

/// The journey's done screen: the check well, the heading block, the
/// three-row summary card, the sample-entry hint and the open button.
/// Built per frame over the wizard state's feed-ins — there is nothing
/// to edit (the button is the only control).
pub(crate) struct DoneStep<S: 'static> {
    /// `state.entry` — the sample entry alone shows the hint.
    entry: Entry,
    /// `state.importResult` — the import entry's summary.
    import_result: Option<ImportSummary>,
    /// `state.connectionName` — the created connection's name.
    connection_name: Option<String>,
    /// `state.aiLabel` — the configured provider's label (`None` when AI
    /// was skipped).
    ai_label: Option<String>,
    /// `state.aiConfigured` — step 2 happened.
    ai_configured: bool,
    /// `onOpen` → the host's `handleOpen` wiring (T114's `on_open`).
    on_open: Option<ActionFn<S>>,
}

impl<S: 'static> DoneStep<S> {
    /// React's mount on S3: the feed-ins at the wizard state's values —
    /// `INITIAL_STATE`'s halves until the host hands the journey's real
    /// state over (the fresh journey is the manual entry with nothing
    /// configured).
    pub(crate) fn new() -> Self {
        DoneStep {
            entry: Entry::Manual,
            import_result: None,
            connection_name: None,
            ai_label: None,
            ai_configured: false,
            on_open: None,
        }
    }

    /// `state.entry`.
    pub(crate) fn entry(mut self, entry: Entry) -> Self {
        self.entry = entry;
        self
    }

    /// `state.importResult` (`None` is React's undefined).
    pub(crate) fn import_result(mut self, result: ImportSummary) -> Self {
        self.import_result = Some(result);
        self
    }

    /// `state.connectionName` (`None` is React's undefined).
    pub(crate) fn connection_name(mut self, name: impl Into<String>) -> Self {
        self.connection_name = Some(name.into());
        self
    }

    /// `state.aiLabel` (`None` is React's `null`).
    pub(crate) fn ai_label(mut self, label: impl Into<String>) -> Self {
        self.ai_label = Some(label.into());
        self
    }

    /// `state.aiConfigured`.
    pub(crate) fn ai_configured(mut self, configured: bool) -> Self {
        self.ai_configured = configured;
        self
    }

    /// `onOpen` → the host's `handleOpen`: stash the sample directive
    /// (T114's `post_onboarding_sample()`), persist the completed
    /// onboarding settings, then fire the window's completion action.
    pub(crate) fn on_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }

    /// Paint the step. `dark` drives the kit tokens.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let summary = connection_summary(
            self.entry,
            self.import_result.as_ref(),
            self.connection_name.as_deref(),
        );

        // React's `connValue`: the imported summary's counts (the
        // imported entry when the run created connections, the updated
        // one when it only replaced), the created connection's name, or
        // the not-configured line.
        let conn_value = match &summary {
            ConnectionSummary::Imported { imported, overwritten, source } => {
                if *imported > 0 {
                    t("onboarding.s3.importedValue")
                        .replace("{count}", &imported.to_string())
                        .replace("{source}", source)
                } else {
                    t("onboarding.s3.updatedValue")
                        .replace("{count}", &overwritten.to_string())
                        .replace("{source}", source)
                }
            }
            ConnectionSummary::Connection { name } => name.clone(),
            ConnectionSummary::None => t("onboarding.s3.notConfigured").to_owned(),
        };

        // React's `aiValue`: the configured label, or the optional
        // not-configured line.
        let ai_value = match (&self.ai_label, self.ai_configured) {
            (Some(label), true) => label.clone(),
            _ => t("onboarding.s3.aiNotConfigured").to_owned(),
        };

        // The root column: `flex flex-col items-center gap-6 py-8`.
        let mut root = col()
            .key("onboarding-step-s3") // React data-testid, verbatim
            .w_full()
            .items_center() // items-center
            .gap(18.0) // gap-6
            .py(24.0) // py-8
            // The check well: `flex h-16 w-16 items-center justify-center
            // rounded-full border border-green/30 bg-green/10 text-green
            // shadow-[0_0_0_8px_rgba(95,208,138,0.04)]` — the 8px (6pt)
            // outer ring in the same green at 4%.
            .child(
                rune::el()
                    .key("onboarding-step-s3-check")
                    .size(48.0, 48.0) // h-16 w-16
                    .center() // items-center justify-center
                    .radius(999.0) // rounded-full (clamped to half the side)
                    .border(0.75, k.c.success.fg.alpha(0.30)) // border border-green/30
                    .fill(k.c.success.fg.alpha(0.10)) // bg-green/10
                    .box_shadow(Shadow {
                        spread: 6.0, // the 8px ring
                        color: k.c.success.fg.alpha(0.04),
                        inset: false,
                        ..Shadow::default()
                    })
                    // The React file's own inline SVG check (h-7 w-7),
                    // in the registry as `ICON_DONE_CHECK`.
                    .child(icon(&ICON_DONE_CHECK, 21.0).color(k.c.success.fg)), // text-green
            )
            // The title block: the `text-center` wrapper around the
            // heading and the subtitle.
            .child(
                col()
                    .key("onboarding-step-s3-heading")
                    .items_center()
                    .child(
                        // The heading: mb-2.5 text-[25px] font-bold
                        // tracking-tight leading-tight text-fg.
                        text(t("onboarding.s3.title").to_owned())
                            .key("onboarding-step-s3-title")
                            .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                            .mb(7.5) // mb-2.5
                            .color(k.c.text_base),
                    )
                    .child(
                        // The subtitle: mx-auto max-w-[48ch]
                        // text-[13.5px] leading-[1.65] text-fg-secondary.
                        text(t("onboarding.s3.subtitle").to_owned())
                            .key("onboarding-step-s3-subtitle")
                            .font(TextStyle::new(10.125, 400.0).lh(16.7))
                            .max_w(SUBTITLE_MAX_W)
                            .wrap()
                            .align(Align::Center) // text-center
                            .color(k.c.text_muted),
                    ),
            )
            // The summary card: `w-full max-w-[480px] overflow-hidden
            // rounded-xl border border-edge bg-surface`.
            .child(
                col()
                    .key("onboarding-step-s3-summary")
                    .w_full()
                    .max_w(SUMMARY_MAX_W) // max-w-[480px]
                    .radius(k.radius.lg) // rounded-xl
                    .clip_rounded(k.radius.lg) // overflow-hidden
                    .border(0.75, k.c.border_base) // border border-edge
                    .fill(k.c.surface_base) // bg-surface
                    .child(summary_row(
                        &k,
                        t("onboarding.s3.connLabel"),
                        &conn_value,
                        matches!(summary, ConnectionSummary::None), // skipped
                        false,
                        "onboarding-summary-connection",
                    ))
                    .child(summary_row(
                        &k,
                        t("onboarding.s3.aiLabel"),
                        &ai_value,
                        !self.ai_configured, // skipped
                        false,
                        "onboarding-summary-ai",
                    ))
                    .child(summary_row(
                        &k,
                        t("onboarding.s3.storageLabel"),
                        t("onboarding.s3.storageValue"),
                        false, // never skipped
                        true,  // last — no bottom border
                        "onboarding-summary-storage",
                    )),
            );

        // The sample hint: only the sample entry, `text-center
        // text-[13px] text-accent`, the 💡 prefix plus the rich
        // `nextHint` with its two bold segments.
        if self.entry == Entry::Sample {
            root = root.child(next_hint(&k));
        }

        // The open button — the journey's only exit.
        let mut open = Button::new(t("onboarding.s3.openBtn").to_owned())
            .variant(ButtonVariant::Contrast) // React primary (T018)
            .control_size(ControlSize::ExtraLarge); // React md (h-9)
        if let Some(on_open) = self.on_open {
            open = open.on_press(move |s: &mut S| on_open(s));
        }
        root.child(open.build(k).key("onboard-open-datazen")) // React data-testid, verbatim
    }
}

impl<S: 'static> Default for DoneStep<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// One summary row: `flex items-center gap-3 px-[18px] py-3 text-[13px]`
/// with the bottom border on every row but the last — the check glyph
/// (`CheckCircle2`, muted when skipped, green otherwise), the fixed-width
/// label and the elided mono value.
fn summary_row(k: &Kit, label: &str, value: &str, skipped: bool, last: bool, testid: &'static str) -> El {
    let mut row = row()
        .key(testid) // React data-testid, verbatim
        .w_full()
        .center_y() // items-center
        .gap(9.0) // gap-3
        .px(13.5) // px-[18px]
        .py(9.0) // py-3
        .child(
            icon(&ICON_CIRCLE_CHECK, 11.25) // h-[15px] w-[15px]
                .key(format!("{testid}-check"))
                .shrink() // shrink-0
                .color(if skipped {
                    k.c.text_muted // text-fg-muted
                } else {
                    k.c.success.fg // text-green
                }),
        )
        .child(
            text(label.to_owned())
                .key(format!("{testid}-label"))
                .w(84.0) // w-[112px]
                .shrink() // shrink-0
                .font(TextStyle::new(9.375, 400.0)) // text-[12.5px]
                .color(k.c.text_muted), // text-fg-secondary
        )
        .child(
            text(value.to_owned())
                .key(format!("{testid}-value"))
                .grow(1.0) // min-w-0 flex-1
                .min_w(0.0)
                .truncate() // overflow-hidden text-ellipsis whitespace-nowrap
                .font(TextStyle::new(9.0, 400.0).mono()) // font-mono text-[12px]
                .color(k.c.text_base), // text-fg
        );
    if !last {
        row = row.border_sides(
            [0.0, 0.0, 0.75, 0.0],
            k.c.border_base.alpha(0.65), // border-b border-edge/65
        );
    }
    row
}

/// The sample-entry hint: `text-center text-[13px] text-accent` — the 💡
/// prefix plus `nextHint` with its `<b>` segments bolded (see
/// [`strip_b_tags`]).
fn next_hint(k: &Kit) -> El {
    let raw = format!("💡 {}", t("onboarding.s3.nextHint"));
    let (plain, bold) = strip_b_tags(&raw);
    let spans: Vec<Span> = bold
        .into_iter()
        .map(|range| Span::new(range.start, range.end).weight(700.0)) // <b>
        .collect();
    rich(plain, spans)
        .key("onboarding-step-s3-hint")
        .font(TextStyle::new(9.75, 400.0)) // text-[13px]
        .wrap()
        .align(Align::Center) // text-center
        .color(k.c.text_accent) // text-accent
}
