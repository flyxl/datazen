//! Port of `packages/frontend/src/windows/onboarding/steps/SampleStep.tsx`
//! (T226): S1 for the sample entry — step 1 of 2 of the first-run journey
//! when the user picks "explore with sample data" on the welcome screen.
//!
//! The React file is the journey's quietest step: there is nothing to pick
//! and no button of its own (the wizard footer's Continue is the way
//! forward). On mount it seeds the bundled SQLite playground once —
//! `seedSampleDb()` (the IPC that copies the bundled demo database out to
//! the user's data dir) and, when no connection named `Sample Playground`
//! exists yet, `saveConnection(buildConnectionConfig({ … sqlite, database:
//! path, group: 'Sample', colorTag: '#4fc3f7' … }))` — and then reports
//! the outcome through `onReady({ path, connectionName })` /
//! `onFailed()`. The panel itself is one status card: the icon well
//! (check-circle once ready, database on failure, the pulsing flask while
//! preparing), the bold status line, the fixed description, the seeded
//! database path, the fixed connection line, and the error line on
//! failure.
//!
//! ## What maps where
//!
//! - `SAMPLE_CONNECTION_NAME` / `SAMPLE_CONNECTION_GROUP` (the seeded
//!   playground connection's fixed identity) are [`SAMPLE_CONNECTION_NAME`]
//!   / [`SAMPLE_CONNECTION_GROUP`] — the host reads the name for its
//!   `SAMPLE_READY` dispatch and the post-onboarding directive (T114's
//!   `connection_name` feed-in); no other React file imports them today.
//! - `seedSamplePlayground()` — the `seedSampleDb` IPC, the
//!   `connections.find((c) => c.name === …)` reuse check (re-entering step
//!   1 must not pile up duplicate connections) and the
//!   `saveConnection(buildConnectionConfig(…))` persist — is ONE host
//!   task (no IPC in the UI crate): the host starts it when the journey
//!   enters the sample entry, guarded by [`SampleStepVm::begin_seeding`]
//!   (the `seedingRef` port — see below), and reports the outcome through
//!   the VM's two promise arms.
//! - `seedingRef` (the once-per-mount promise guard): React keeps the
//!   promise itself as the guard so a StrictMode remount re-observes the
//!   first run instead of re-running it. The port's guard is
//!   [`SampleStepVm::begin_seeding`] — it returns `true` only the first
//!   call, so the host runs its seeding task exactly once per entry into
//!   the sample step, however many frames rebuild the step.
//! - The effect's `.then` / `.catch` arms are [`SampleStepVm::
//!   seed_succeeded`] / [`SampleStepVm::seed_failed`] (the `setState`
//!   halves — path + ready, or failed + the error message; React's
//!   `e.message` / `String(e)` normalization is the host task's catch
//!   arm, which reports an already-string message). The `onReady` /
//!   `onFailed` prop halves are the host's own dispatches, run right
//!   after the VM arm: `SAMPLE_READY { path, connectionName:
//!   SAMPLE_CONNECTION_NAME }` and `SAMPLE_FAILED` (the T323 machine,
//!   whose outputs feed T114's wizard seams). React's `cancelled` flag is
//!   the unmount concern the host task owns.
//! - `status` / `samplePath` / `error` (the three `useState`s) are
//!   [`SampleStepVm`], kept in the host's `S` and cloned per frame (the
//!   T103 `FormBodyVm` discipline).
//! - `readyConnectionName` — the wizard passes the existing connection
//!   name only when `state.samplePath` is already set (step 1 completed
//!   once, then re-entered) — is [`SampleStep::ready_connection_name`],
//!   and `ready = status === 'ready' || Boolean(readyConnectionName)`
//!   gates the green chrome. The LABEL keeps following the local status
//!   alone, exactly as React's nested ternary does: re-entering shows the
//!   ready chrome (via the prop) while the fresh mount still says
//!   "preparing" until its own seeding resolves.
//! - i18n: the seven reads (`onboarding.s1.stepLabel` — via the shared
//!   tag — `sampleTitle`, `sampleSubtitle`, `samplePreparing`,
//!   `sampleReady`, `sampleReadyDesc`, `sampleFailed`) port to
//!   `crate::i18n::t` with the same keys; `sampleConnection`'s `{name}`
//!   placeholder is formatted with [`SAMPLE_CONNECTION_NAME`], exactly as
//!   React's `t(…, { name })` does.
//! - Icons: the registry's `ICON_CIRCLE_CHECK` (lucide `circle-check-big`
//!   — the icon React 0.487 exports as `CheckCircle2`, the T107 ruling),
//!   `ICON_CONNECTIONS` (the registry's lucide `database` body) and
//!   `ICON_FLASK_CONICAL`. No new registry entries.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the header's derived keys sit
//! under the root's `onboarding-step-s1-sample-` prefix and the status
//! card's parts under React's own `onboarding-sample-` testid prefix (the
//! manual step's ruling — the shared tag carries its own
//! `onboarding-step-tag-` prefix):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s1-sample"`) | `onboarding-step-s1-sample` |
//! | heading `h2` | `onboarding-step-s1-sample-title` |
//! | subtitle `p` | `onboarding-step-s1-sample-subtitle` |
//! | status card (`data-testid="onboarding-sample-status"`) | `onboarding-sample-status` |
//! | the card's icon well | `onboarding-sample-status-icon` |
//! | the card's text column | `onboarding-sample-status-text` |
//! | the bold status line | `onboarding-sample-status-label` |
//! | the description line | `onboarding-sample-status-desc` |
//! | the path line (`data-testid="onboarding-sample-path"`) | `onboarding-sample-path` |
//! | the connection line | `onboarding-sample-connection` |
//! | the error line (`data-testid="onboarding-sample-error"`) | `onboarding-sample-error` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); `max-w-[56ch]` on the subtitle becomes 283.5pt (the `ch` unit
//! approximation, the `ai_provider_step.rs` ruling); `rounded-xl` on the
//! card is the kit's `lg` token (the `workspace_default_cards.rs`
//! mapping) while the well's `rounded-lg` is the exact 8px → 6pt;
//! `animate-pulse` on the preparing flask is `LoopSpec::opacity(2.0, 0.0,
//! 0.5, 1.0)` (the `chat_bubble.rs` ruling — Tailwind's 2s opacity
//! 1 → 0.5 → 1 loop); the path line's `title={samplePath}` is `.help(`
//! the path `)` (the `column_mapping_row.rs` ruling); `select-text` on
//! the error line has no Rune counterpart (the `main_page.rs` ruling —
//! Rune text carries no selection lock); `truncate` is the text elision.

// The onboarding wizard host (T114's `sample` seat) is the production
// consumer; until the window track wires it, this crate's audit is the
// only caller.
#![allow(dead_code)]

use rune::{col, icon, row, text, El, LoopSpec, TextStyle};

use crate::i18n::t;
use crate::icons::{ICON_CIRCLE_CHECK, ICON_CONNECTIONS, ICON_FLASK_CONICAL};
use crate::ui::kit::kit;

use super::step_tag::step_tag;

/// React `SAMPLE_CONNECTION_NAME`: the fixed identity of the seeded
/// playground connection.
pub const SAMPLE_CONNECTION_NAME: &str = "Sample Playground";

/// React `SAMPLE_CONNECTION_GROUP`: the group the seeded playground
/// connection is filed under.
pub const SAMPLE_CONNECTION_GROUP: &str = "Sample";

/// React `max-w-[640px]` (640px → 480pt): the step column's cap (the
/// welcome step's own cap — both S1 panels are 640px columns).
pub const STEP_MAX_W: f32 = 480.0;

/// React `max-w-[56ch]` on the subtitle: 56 × the "0" advance (≈ 0.5em at
/// 13.5px → 6.75px) ≈ 378px → 283.5pt. Approximate by construction — the
/// `ch` unit has no Rune counterpart (the `ai_provider_step.rs` ruling).
const SUBTITLE_MAX_W: f32 = 283.5;

/// React `SampleStatus`: the seeding journey's three states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SampleStatus {
    /// `'preparing'` — the seeding task is in flight (the initial state).
    #[default]
    Preparing,
    /// `'ready'` — the playground is seeded and the connection saved.
    Ready,
    /// `'failed'` — the seeding task rejected (the error line shows why).
    Failed,
}

/// The half of React's state this file itself owns: the seeding journey's
/// three `useState`s plus the `seedingRef` guard. The host keeps this in
/// its `S` and clones it per frame, exactly the T103 `FormBodyVm`
/// discipline.
#[derive(Clone, Debug, Default)]
pub struct SampleStepVm {
    /// `status` — the seeding journey's state (React's `'preparing'`
    /// initial value is the default).
    pub status: SampleStatus,
    /// `samplePath` — the seeded database's path (`''` until the journey
    /// succeeds; the path line renders only when non-empty).
    pub sample_path: String,
    /// `error` — the failure message (`null` until the journey fails).
    pub error: Option<String>,
    /// The `seedingRef` guard: whether the seeding task has been started
    /// for this entry into the step. Never read by the tree — it is the
    /// host's run-once rule (see [`Self::begin_seeding`]).
    seeding_started: bool,
}

impl SampleStepVm {
    /// React's `seedingRef.current ??= seedSamplePlayground()` guard:
    /// returns `true` only the first call, so the host starts its seeding
    /// task exactly once per entry into the sample step — a re-built step
    /// (React's StrictMode remount, or any re-render) re-observes the one
    /// run instead of re-running it.
    pub fn begin_seeding(&mut self) -> bool {
        if self.seeding_started {
            return false;
        }
        self.seeding_started = true;
        true
    }

    /// The effect's `.then` arm's `setState` half: `setSamplePath(path);
    /// setStatus('ready')`. The host runs this when its seeding task
    /// resolves, then dispatches `SAMPLE_READY { path, connectionName:
    /// SAMPLE_CONNECTION_NAME }` (the `onReady` prop's half).
    pub fn seed_succeeded(&mut self, path: impl Into<String>) {
        self.sample_path = path.into();
        self.status = SampleStatus::Ready;
    }

    /// The effect's `.catch` arm's `setState` half: `setStatus('failed');
    /// setError(message)`. The host runs this when its seeding task
    /// rejects (with the already-normalized message string), then
    /// dispatches `SAMPLE_FAILED` (the `onFailed` prop's half).
    pub fn seed_failed(&mut self, message: impl Into<String>) {
        self.status = SampleStatus::Failed;
        self.error = Some(message.into());
    }
}

/// The journey's sample-entry step: the tag, the heading, the subtitle and
/// the one status card tracking the seeding journey. Built per frame over
/// the host's [`SampleStepVm`] — there is nothing to click (the wizard
/// footer's Continue is the way forward), so no action slots exist.
pub struct SampleStep {
    /// The step-local state (this file's own `useState`s plus the guard).
    vm: SampleStepVm,
    /// `readyConnectionName` — the existing connection name when step 1
    /// was already completed once (the wizard passes it only when
    /// `state.samplePath` is set).
    ready_connection_name: Option<String>,
}

impl SampleStep {
    /// React's mount on S1-sample: the fresh journey's `preparing` state,
    /// no `readyConnectionName` prop.
    pub fn new(vm: SampleStepVm) -> Self {
        SampleStep {
            vm,
            ready_connection_name: None,
        }
    }

    /// `readyConnectionName` — `None` is React's undefined (the prop is
    /// absent until the wizard has a seeded path to hand over).
    pub fn ready_connection_name(mut self, name: Option<String>) -> Self {
        self.ready_connection_name = name;
        self
    }

    /// Paint the step. `dark` drives the kit tokens.
    pub fn build(&self, dark: bool) -> El {
        let k = kit(dark);

        // React's `ready`: the local status, OR the already-completed-once
        // prop. The chrome (card, well, glyph) reads this; the label keeps
        // reading the local status alone, exactly the nested ternary's
        // split.
        let ready = self.vm.status == SampleStatus::Ready || self.ready_connection_name.is_some();

        // The icon well's chrome: `border-green/25 bg-green/12 text-green`
        // once ready, `border-accent/18 bg-accent/12 text-accent`
        // otherwise (both the failed and the preparing states).
        let (well_border, well_fill, well_ink) = if ready {
            (
                k.c.success.fg.alpha(0.25), // border-green/25
                k.c.success.fg.alpha(0.12), // bg-green/12
                k.c.success.fg,             // text-green
            )
        } else {
            (
                k.c.bg_accent.alpha(0.18), // border-accent/18
                k.c.bg_accent.alpha(0.12), // bg-accent/12
                k.c.text_accent,           // text-accent
            )
        };

        // The glyph: CheckCircle2 once ready (the prop wins over a local
        // failure too, as in React), Database on failure, the pulsing
        // FlaskConical while preparing.
        let glyph = if ready {
            &ICON_CIRCLE_CHECK
        } else if self.vm.status == SampleStatus::Failed {
            &ICON_CONNECTIONS
        } else {
            &ICON_FLASK_CONICAL
        };
        // The glyph's key carries the status: React swaps the icon COMPONENT
        // (FlaskConical → CheckCircle2), which remounts and drops the pulse
        // animation; an unkeyed same-position element here would keep the
        // running loop attached after the swap.
        let glyph_key = format!(
            "onboarding-sample-status-glyph-{}",
            if ready {
                "ready"
            } else if self.vm.status == SampleStatus::Failed {
                "failed"
            } else {
                "preparing"
            }
        );
        let mut glyph_el = icon(glyph, 13.5) // h-[18px] w-[18px]
            .key(glyph_key)
            .color(well_ink);
        // `animate-pulse` rides the preparing flask alone — Tailwind's 2s
        // opacity 1 → 0.5 → 1 loop (the `chat_bubble.rs` ruling); the
        // check-circle and database glyphs render static.
        if !ready && self.vm.status != SampleStatus::Failed {
            glyph_el = glyph_el.looping(LoopSpec::opacity(2.0, 0.0, 0.5, 1.0));
        }

        // The text column: `min-w-0 flex-1`, React's four (or five) lines.
        let mut text_col = col()
            .key("onboarding-sample-status-text")
            .grow(1.0) // flex-1
            .min_w(0.0) // min-w-0
            // <b>: block text-[13.5px] font-semibold tracking-tight text-fg
            // — the label follows the LOCAL status alone (see `ready`).
            .child(
                text(
                    match self.vm.status {
                        SampleStatus::Preparing => t("onboarding.s1.samplePreparing"),
                        SampleStatus::Failed => t("onboarding.s1.sampleFailed"),
                        SampleStatus::Ready => t("onboarding.s1.sampleReady"),
                    }
                    .to_owned(),
                )
                .key("onboarding-sample-status-label")
                .font(TextStyle::new(10.125, 600.0).tracking(-0.253125))
                .color(k.c.text_base), // text-fg
            )
            // The description: mt-0.5 block text-[12.5px] leading-relaxed
            // text-fg-secondary.
            .child(
                text(t("onboarding.s1.sampleReadyDesc").to_owned())
                    .key("onboarding-sample-status-desc")
                    .font(TextStyle::new(9.375, 400.0).lh(15.23))
                    .mt(1.5) // mt-0.5
                    .color(k.c.text_muted), // text-fg-secondary
            );
        // The path line — only once the journey has a path to show.
        if !self.vm.sample_path.is_empty() {
            text_col = text_col.child(
                text(self.vm.sample_path.clone())
                    .key("onboarding-sample-path") // React data-testid, verbatim
                    .font(TextStyle::new(8.625, 400.0).mono()) // font-mono text-[11.5px]
                    .mt(3.0) // mt-1
                    .truncate() // truncate
                    .color(k.c.text_muted) // text-fg-muted
                    // title={samplePath} — the full path on hover.
                    .help(self.vm.sample_path.clone()),
            );
        }
        // The connection line: mt-1 block font-mono text-[11.5px]
        // text-fg-muted, the {name} placeholder taking the fixed identity.
        text_col = text_col.child(
            text(t("onboarding.s1.sampleConnection").replace("{name}", SAMPLE_CONNECTION_NAME))
                .key("onboarding-sample-connection")
                .font(TextStyle::new(8.625, 400.0).mono())
                .mt(3.0) // mt-1
                .color(k.c.text_muted), // text-fg-muted
        );
        // The error line — only on failure.
        if let Some(error) = &self.vm.error {
            text_col = text_col.child(
                text(error.clone())
                    .key("onboarding-sample-error") // React data-testid, verbatim
                    .font(TextStyle::new(9.0, 400.0)) // text-[12px]
                    .mt(3.0) // mt-1
                    .color(k.c.danger.fg), // text-red
            );
        }

        // The status card: `flex items-start gap-3 rounded-xl border p-4`,
        // `border-green/25 bg-green/8` once ready, `border-edge
        // bg-surface` otherwise (items-start is the row's default top
        // alignment — the `ai_provider_step.rs` ruling).
        let card = row()
            .key("onboarding-sample-status") // React data-testid, verbatim
            .w_full()
            .gap(9.0) // gap-3
            .radius(k.radius.lg) // rounded-xl
            .px(12.0) // p-4
            .py(12.0)
            .border(
                0.75,
                if ready {
                    k.c.success.fg.alpha(0.25) // border-green/25
                } else {
                    k.c.border_base // border-edge
                },
            )
            .fill(if ready {
                k.c.success.fg.alpha(0.08) // bg-green/8
            } else {
                k.c.surface_base // bg-surface
            })
            // The icon well: `flex h-9 w-9 shrink-0 items-center
            // justify-center rounded-lg border`.
            .child(
                rune::el()
                    .key("onboarding-sample-status-icon")
                    .size(27.0, 27.0) // h-9 w-9
                    .shrink() // shrink-0
                    .center() // items-center justify-center
                    .radius(6.0) // rounded-lg (8px → 6pt, the brand_sidebar ruling)
                    .border(0.75, well_border)
                    .fill(well_fill)
                    .child(glyph_el),
            )
            .child(text_col);

        col()
            .key("onboarding-step-s1-sample") // React data-testid, verbatim
            .w_full()
            .max_w(STEP_MAX_W) // max-w-[640px]
            // <StepTag label={t('onboarding.s1.stepLabel')} />
            .child(step_tag(&k, t("onboarding.s1.stepLabel")))
            // The heading: mb-2.5 text-[25px] font-bold tracking-tight
            // leading-tight text-fg (the siblings' metrics).
            .child(
                text(t("onboarding.s1.sampleTitle").to_owned())
                    .key("onboarding-step-s1-sample-title")
                    .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                    .mb(7.5) // mb-2.5
                    .color(k.c.text_base),
            )
            // The subtitle: mb-7 max-w-[56ch] text-[13.5px] leading-[1.65]
            // text-fg-secondary.
            .child(
                text(t("onboarding.s1.sampleSubtitle").to_owned())
                    .key("onboarding-step-s1-sample-subtitle")
                    .font(TextStyle::new(10.125, 400.0).lh(16.7))
                    .max_w(SUBTITLE_MAX_W)
                    .mb(21.0) // mb-7
                    .color(k.c.text_muted),
            )
            .child(card)
    }
}
