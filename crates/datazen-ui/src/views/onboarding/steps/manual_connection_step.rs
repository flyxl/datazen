//! Port of `packages/frontend/src/windows/onboarding/steps/
//! ManualConnectionStep.tsx` (T203): S1 for the manual entry — step 1 of 2
//! of the first-run journey when the user picks "manually create a
//! connection" on the welcome screen.
//!
//! The React file is a thin frame around the shared connection form: the
//! step tag and heading, a bordered card that mirrors the
//! `NewConnectionDialog` layout — a 220px driver sidebar (search + the
//! popularity-ordered database-type buttons with their `DbTypeBadge`
//! tiles) on the left, the `ConnectionFormBody` (window variant) in a
//! scroll area on the right — and a Test/Save footer whose Save button is
//! the journey's ONLY way forward (`disabled={!tested}` — the wizard
//! footer hides Continue for the manual entry for exactly this reason).
//!
//! ## What maps where
//!
//! - `useConnectionForm({ defaultGroup: null, existingConnections,
//!   onAfterSave })` is the form machine — a host/store concern (no IPC in
//!   the UI crate). The port renders it through the T103
//!   [`ConnectionFormBody`] seam: the host hands a [`FormBodyVm`] snapshot
//!   per frame ([`ManualConnectionStep::form`]) plus the two ports back
//!   into `S` ([`ManualConnectionStep::on_form_vm`] for the text fields,
//!   [`ManualConnectionStep::on_action`] for the discrete actions). The
//!   step itself pins exactly the props React hands the body —
//!   `variant="window"` becomes `is_window = true` on the rendered
//!   snapshot, and `showDbTypeSelect` / `groupOptions` (not passed →
//!   `false` / `undefined`) are pinned to `false` / `None` — so the
//!   sidebar is the only db-type picker, as in React.
//! - The driver sidebar's `onClick={() =>
//!   form.handleDatabaseTypeChange(db.value)}` routes through the SAME
//!   [`FormAction::DatabaseType`] action the T103 body's own select
//!   dispatches — one seam, one host translation.
//! - `useConnectionStore` (`connections` / `fetchConnections` /
//!   `fetchGroups`) and the mount effect's
//!   `connectionCommands.getAvailableDrivers()` are host loads when the
//!   journey reaches this step: the loaded list arrives through
//!   [`ManualStepVm::available_drivers`], where `None` covers all three of
//!   React's null states — not loaded yet, load in flight, and load failed
//!   (the `.catch(() => setAvailableDrivers(null))` arm) — and in all
//!   three the UNFILTERED type list shows, exactly as React's
//!   `!availableDrivers ? ALL_DB_TYPES : …` branch.
//! - `useConnectionClipboardFill(form, { enabled: true, autoRead: false,
//!   availableTypes: availableDrivers })` stays host-side: clipboard reads
//!   are host IPC, and the journey never reads the clipboard on its own —
//!   the read needs a user gesture on macOS and otherwise freezes the form
//!   (and anything driving the window) until the paste prompt is answered;
//!   explicit pastes keep working through the host's paste path. The
//!   `availableTypes` filter rides the same host seam.
//! - Step-local state (`driverQuery`, `tested`, `testing`, `testResult`)
//!   is [`ManualStepVm`] — the half of React's state this file itself
//!   owns — kept in the host's `S` behind
//!   [`ManualConnectionStep::on_step`]. `handleTest`'s transitions are
//!   the VM's methods ([`ManualStepVm::begin_test`] /
//!   [`ManualStepVm::test_succeeded`] / [`ManualStepVm::test_failed`]):
//!   the host calls `begin_test` when the footer's Test button fires its
//!   [`ManualConnectionStep::on_test`] slot (which wraps the form
//!   machine's `onTest`), then reports the promise's outcome through the
//!   two result methods — `tested` falls back to `false` on failure, so a
//!   failed test re-locks Save, and a new test always clears the result
//!   line first (`setTestResult(null)`), all exactly React's ordering.
//! - `nameRef` + the `onSaved(connectionName)` prop: the save completion
//!   seam. The footer's Save fires [`ManualConnectionStep::on_save`] (the
//!   form machine's `onSave`); when the host's save resolves it runs its
//!   own `onAfterSave` → the wizard's `onSaved` dispatch, reading the
//!   connection name from the same form machine (React's `nameRef.current`
//!   is just the form's live `name` at save time — the host's `FormBodyVm`
//!   carries it). The wizard's `SAVE_SUCCESS` transition is T114's.
//! - `ALL_DB_TYPES` / `sortDbTypesByPopularity` /
//!   `filterDbTypesByQuery` are the pure halves this file composes:
//!   [`DB_TYPE_POPULARITY_ORDER`] + [`sort_db_types_by_popularity`] +
//!   [`filter_db_types_by_query`] + [`all_db_types`]. The registry itself
//!   (`DB_REGISTRY`, build-time generated from the driver metas) has no
//!   Rune port yet, so [`DB_REGISTRY_STANDIN`] lists the 22 db types of
//!   the drivers shipped in this repo with their `meta.ts` labels —
//!   removal stage: when the `lib/databaseTypes.ts` port lands, derive the
//!   list from it (the four popularity ids absent from the shipped set —
//!   kiwi / presto / trino / superset — then simply do not appear, like a
//!   reduced-driver build in React).
//! - The subtitle's `dangerouslySetInnerHTML`: every locale's
//!   `onboarding.s1.subtitle` carries exactly one `<b>…</b>` segment, so
//!   the port strips the tags and bolds the inner byte range with a
//!   [`Span`] ([`split_b_tags`]) — same visible text, same emphasis.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; this step's own derived keys
//! sit under the one `onboarding-step-s1-manual-` prefix, and the two
//! per-driver parts hang off React's own `onboard-driver-<value>` testid:
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s1-manual"`) | `onboarding-step-s1-manual` |
//! | heading `h2` | `onboarding-step-s1-manual-title` |
//! | subtitle `p` | `onboarding-step-s1-manual-subtitle` |
//! | the bordered form card (`rounded-xl border border-edge`) | `onboarding-step-s1-manual-frame` |
//! | the driver sidebar `aside` | `onboarding-step-s1-manual-sidebar` |
//! | the sidebar's search box (`p-3`) | `onboarding-step-s1-manual-driver-search` |
//! | the search `Input` | `onboarding-step-s1-manual-driver-search-input` |
//! | the scrollable driver list | `onboarding-step-s1-manual-driver-list` |
//! | the no-match line | `onboarding-step-s1-manual-driver-empty` |
//! | one driver button (`data-testid={`onboard-driver-${db.value}`}`) | `onboard-driver-<value>` |
//! | that button's `DbTypeBadge` | `onboard-driver-<value>-badge` |
//! | that button's label `span` | `onboard-driver-<value>-label` |
//! | the right form column `main` | `onboarding-step-s1-manual-main` |
//! | the form scroll area (`p-6 overflow-y-auto`) | `onboarding-step-s1-manual-form` |
//! | the Test/Save `footer` | `onboarding-step-s1-manual-footer` |
//! | the result `span` | `onboarding-step-s1-manual-test-result` |
//! | test button (`data-testid="onboard-test-connection"`) | `onboard-test-connection` |
//! | save button (`data-testid="onboard-s1-save"`) | `onboard-s1-save` |
//!
//! The form body's own keys are T103's (`new-conn-*`, the
//! `connection_form_body.rs` table). Known deltas (documented, not
//! silent): pixel classes become points (×0.75); `max-w-[56ch]` on the
//! subtitle becomes 283.5pt (the `ch` approximation, the
//! `ai_provider_step.rs` ruling); the frame's `overflow-hidden` is
//! `clip_rounded` at the card's own radius; the kit `Button` chrome
//! (radius/padding/icon metrics) follows the kit's ExtraLarge skin, not
//! Tailwind's `rounded-[9px] px-[18px]`/`h-3.5` glyph (the
//! `wizard_footer.rs` ruling); the driver buttons' `hover:text-fg` ink
//! hover has no Rune counterpart (`El::hover` is a fill hover — the
//! `hover:bg-accent/6` half is kept); `transition-colors` /
//! `select-none` are static; `localeCompare` in the popularity sort's
//! tiebreak is byte order (the ids are ASCII); `autoFocus` on the name
//! field (window variant) has no Rune counterpart and is T103's
//! documented delta.

// The onboarding wizard host (T114's `manual` seat) is the production
// consumer; until the window track wires it, this crate's audit is the
// only caller.
#![allow(dead_code)]

use std::rc::Rc;
use std::sync::OnceLock;

use rune::{col, rich, row, text, El, ScrollSpec, Span, TextEdit, TextStyle};

use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::connection::connection_form_body::{
    ConnectionFormBody, FormAction, FormBodyVm,
};
use crate::components::db_type_badge::DbTypeBadge;
use crate::i18n::t;
use crate::icons::ICON_FLASK_CONICAL;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::step_tag::step_tag;

/// React `max-w-[56ch]` on the subtitle: 56 × the "0" advance (≈ 0.5em at
/// 13.5px → 6.75px) ≈ 378px → 283.5pt. Approximate by construction — the
/// `ch` unit has no Rune counterpart (the `ai_provider_step.rs` ruling).
const SUBTITLE_MAX_W: f32 = 283.5;

/// React `w-[220px]` (220px → 165pt): the driver sidebar's fixed width.
const SIDEBAR_W: f32 = 165.0;

/// The step's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;
/// The step-local VM accessor (the `FormBodyVm` convention, T103).
type StepVmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ManualStepVm + 'static>;
/// The form machine's VM accessor, forwarded to the T103 body.
type FormVmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut FormBodyVm + 'static>;
/// The form machine's action port, forwarded to the T103 body.
type FormActionFn<S> = Rc<dyn Fn(&mut S, FormAction) + 'static>;
/// The search field's derived `TextEdit` accessor (the T103 `EditFn`).
type QueryEditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// React `testResult: 'ok' | 'fail' | null` — the footer result line's
/// three states (`None` is React's `null`: no line text, muted ink).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestOutcome {
    /// `'ok'` — `✓` + the success pack entry, green ink.
    Ok,
    /// `'fail'` — `✕` + the failure pack entry, red ink.
    Fail,
}

/// The half of React's state this file itself owns (everything except the
/// form machine): the driver search query, the available-driver list and
/// the test footer's three flags. The host keeps this in its `S` and
/// clones it per frame, exactly the T103 `FormBodyVm` discipline.
#[derive(Clone, Debug)]
pub struct ManualStepVm {
    /// `driverQuery` — the sidebar search box's edit buffer.
    pub driver_query: TextEdit,
    /// `availableDrivers` — `null` while pending AND on load failure
    /// (React's `.catch(() => setAvailableDrivers(null))`); the unfiltered
    /// list shows in all those states.
    pub available_drivers: Option<Vec<String>>,
    /// `tested` — the Save gate (`disabled={!tested}`).
    pub tested: bool,
    /// `testing` — the Test button's busy flag (`disabled={testing}`).
    pub testing: bool,
    /// `testResult` — the footer result line.
    pub test_result: Option<TestOutcome>,
}

impl Default for ManualStepVm {
    /// React's `useState` initial values: `''`, `null`, `false`, `false`,
    /// `null`.
    fn default() -> Self {
        ManualStepVm {
            driver_query: TextEdit::new(String::new()),
            available_drivers: None,
            tested: false,
            testing: false,
            test_result: None,
        }
    }
}

impl ManualStepVm {
    /// React `handleTest`'s opening two lines: `setTesting(true);
    /// setTestResult(null)` — a new test always clears the result line
    /// before it resolves.
    pub fn begin_test(&mut self) {
        self.testing = true;
        self.test_result = None;
    }

    /// The success arm plus `finally`: `setTested(true);
    /// setTestResult('ok')` … `setTesting(false)`.
    pub fn test_succeeded(&mut self) {
        self.tested = true;
        self.test_result = Some(TestOutcome::Ok);
        self.testing = false;
    }

    /// The catch arm plus `finally`: `setTested(false);
    /// setTestResult('fail')` … `setTesting(false)` — a failed test
    /// re-locks the Save button.
    pub fn test_failed(&mut self) {
        self.tested = false;
        self.test_result = Some(TestOutcome::Fail);
        self.testing = false;
    }

    /// React's `dbTypes` memo: `!availableDrivers ? ALL_DB_TYPES :
    /// ALL_DB_TYPES.filter((db) => availableDrivers.includes(db.value))`,
    /// then `filterDbTypesByQuery(available, driverQuery)`.
    pub fn db_types(&self) -> Vec<(&'static str, &'static str)> {
        let available: Vec<(&'static str, &'static str)> = match &self.available_drivers {
            None => all_db_types().to_vec(),
            Some(drivers) => all_db_types()
                .iter()
                .copied()
                .filter(|(value, _)| drivers.iter().any(|d| d.as_str() == *value))
                .collect(),
        };
        filter_db_types_by_query(&available, &self.driver_query.text)
    }
}

/// React `DB_TYPE_POPULARITY_ORDER` (`lib/databaseTypes.ts:31`): the
/// new-connection sidebar order — most commonly used types first, not
/// wire-protocol family. Types absent from this list sort after, by id.
pub const DB_TYPE_POPULARITY_ORDER: [&str; 26] = [
    "mysql",
    "postgresql",
    "mariadb",
    "sqlite",
    "redis",
    "mongodb",
    "sqlserver",
    "clickhouse",
    "doris",
    "starrocks",
    "ob_oracle",
    "elasticsearch",
    "duckdb",
    "questdb",
    "cloudberry",
    "turso",
    "rqlite",
    "influxdb",
    "victoriametrics",
    "hbase",
    "manticore",
    "vector",
    "kiwi",
    "presto",
    "trino",
    "superset",
];

/// React `dbTypePopularityRank`'s lookup: the id's rank, or
/// `Number.MAX_SAFE_INTEGER` when the id is absent from the order.
fn db_type_popularity_rank(id: &str) -> usize {
    DB_TYPE_POPULARITY_ORDER
        .iter()
        .position(|candidate| *candidate == id)
        .unwrap_or(usize::MAX)
}

/// React `sortDbTypesByPopularity` (`lib/databaseTypes.ts:63`): rank order,
/// ties broken by id (`localeCompare` — byte order here; the ids are
/// ASCII). A stable-equivalent total order, so the input order cannot leak
/// through.
pub fn sort_db_types_by_popularity(
    mut items: Vec<(&'static str, &'static str)>,
) -> Vec<(&'static str, &'static str)> {
    items.sort_by(|a, b| {
        db_type_popularity_rank(a.0)
            .cmp(&db_type_popularity_rank(b.0))
            .then_with(|| a.0.cmp(b.0))
    });
    items
}

/// React `filterDbTypesByQuery` (`lib/filterDbTypes.ts:1`): the trimmed,
/// lower-cased query matches the label OR the value (both lower-cased); an
/// empty query keeps everything.
pub fn filter_db_types_by_query<'a>(
    items: &[(&'a str, &'a str)],
    query: &str,
) -> Vec<(&'a str, &'a str)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .copied()
        .filter(|(value, label)| {
            label.to_lowercase().contains(&q) || value.to_lowercase().contains(&q)
        })
        .collect()
}

/// The `DB_REGISTRY` stand-in: the 22 db types of the drivers shipped in
/// this repo, with their `meta.ts` labels, in registry-iteration (driver
/// crate) order — the sort applies the popularity order, as in React.
/// Removal stage: when the `lib/databaseTypes.ts` port lands, derive this
/// from the ported registry instead.
const DB_REGISTRY_STANDIN: [(&str, &str); 22] = [
    ("postgresql", "PostgreSQL"),
    ("questdb", "QuestDB"),
    ("cloudberry", "Cloudberry"),
    ("mysql", "MySQL"),
    ("mariadb", "MariaDB"),
    ("doris", "Doris"),
    ("starrocks", "StarRocks"),
    ("manticore", "ManticoreSearch"),
    ("ob_oracle", "OceanBase (Oracle)"),
    ("sqlite", "SQLite"),
    ("redis", "Redis"),
    ("mongodb", "MongoDB"),
    ("sqlserver", "SQL Server"),
    ("clickhouse", "ClickHouse"),
    ("duckdb", "DuckDB"),
    ("elasticsearch", "Elasticsearch"),
    ("hbase", "HBase"),
    ("influxdb", "InfluxDB"),
    ("rqlite", "RQLite"),
    ("turso", "Turso / libSQL"),
    ("vector", "Vector DB"),
    ("victoriametrics", "VictoriaMetrics"),
];

/// React's module-scope `ALL_DB_TYPES`: the registry's `(value, label)`
/// pairs, popularity-sorted once at module load. Computed once here (the
/// sort is the React module's own init work, not per-render).
pub fn all_db_types() -> &'static [(&'static str, &'static str)] {
    static SORTED: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    SORTED.get_or_init(|| {
        sort_db_types_by_popularity(DB_REGISTRY_STANDIN.iter().copied().collect())
    })
}

/// The subtitle's `dangerouslySetInnerHTML` handling: strip the one
/// `<b>…</b>` pair and report the inner byte range for the bold [`Span`].
/// A string with no (complete) pair comes back untouched and unbolded —
/// future locales without the tag degrade to plain text.
fn split_b_tags(raw: &str) -> (String, Option<std::ops::Range<usize>>) {
    let Some(open) = raw.find("<b>") else {
        return (raw.to_owned(), None);
    };
    let after = &raw[open + 3..];
    let Some(close) = after.find("</b>") else {
        return (raw.to_owned(), None);
    };
    let plain = format!("{}{}{}", &raw[..open], &after[..close], &after[close + 4..]);
    (plain, Some(open..open + close))
}

/// The journey's manual-entry step: the frame around the shared connection
/// form. Built per frame over the host's two VMs — the step's own
/// [`ManualStepVm`] and the form machine's [`FormBodyVm`] (T103) — with
/// the action slots the host wires to its loads, dispatches and save/test
/// flows.
pub struct ManualConnectionStep<S: 'static> {
    /// The step-local half of React's state (this file's own `useState`s).
    step: ManualStepVm,
    /// The form machine's snapshot (the `useConnectionForm` return value,
    /// T103's `FormBodyVm`).
    form: FormBodyVm,
    /// The step-local VM accessor in `S` (the search field's edits).
    on_step: Option<StepVmFn<S>>,
    /// The form machine's VM accessor, forwarded to the T103 body.
    on_form_vm: Option<FormVmFn<S>>,
    /// The form machine's action port, forwarded to the T103 body AND used
    /// by the driver sidebar's buttons (`handleDatabaseTypeChange`).
    on_action: Option<FormActionFn<S>>,
    /// `handleTest` — the host's test flow around the form's `onTest`.
    on_test: Option<ActionFn<S>>,
    /// `form.onSave()` — the host's save flow (whose completion runs the
    /// host's `onAfterSave` → the wizard's `onSaved` dispatch).
    on_save: Option<ActionFn<S>>,
}

impl<S: 'static> ManualConnectionStep<S> {
    /// React's mount on S1-manual: both VMs at their initial state, no
    /// seams wired until the host hands them over.
    pub fn new(step: ManualStepVm, form: FormBodyVm) -> Self {
        ManualConnectionStep {
            step,
            form,
            on_step: None,
            on_form_vm: None,
            on_action: None,
            on_test: None,
            on_save: None,
        }
    }

    /// The step-local VM accessor (`driverQuery`'s edits land in `S`).
    pub fn on_step(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut ManualStepVm + 'static,
    ) -> Self {
        self.on_step = Some(Rc::new(f));
        self
    }

    /// The form machine's VM accessor — the T103 `on_vm` seam, forwarded.
    pub fn on_form_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut FormBodyVm + 'static,
    ) -> Self {
        self.on_form_vm = Some(Rc::new(f));
        self
    }

    /// The form machine's action port — the T103 `on_action` seam,
    /// forwarded (and shared with the driver sidebar's buttons).
    pub fn on_action(mut self, f: impl Fn(&mut S, FormAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// The footer's Test button → React `handleTest` (the host wraps the
    /// form's `onTest`; the outcome reports back through the VM's
    /// `test_succeeded` / `test_failed`).
    pub fn on_test(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_test = Some(Rc::new(f));
        self
    }

    /// The footer's Save button → `form.onSave()` (whose completion runs
    /// the host's `onAfterSave` → the wizard's `onSaved` dispatch, with
    /// the saved name from the form machine — React's `nameRef.current`).
    pub fn on_save(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }

    /// Paint the step. `dark` drives the kit tokens; `layer` receives the
    /// open listboxes of the composed form body (the T103 convention).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let ManualConnectionStep {
            step,
            mut form,
            on_step,
            on_form_vm,
            on_action,
            on_test,
            on_save,
        } = self;

        // React pins the body's props here: `variant="window"`, no
        // `showDbTypeSelect`, no `groupOptions` (the sidebar is the only
        // db-type picker, and the advanced accordion's group select gets
        // no options).
        form.is_window = true;
        form.show_db_type_select = false;
        form.group_options = None;

        // The search field's edit accessor, derived from the step-VM one
        // (the T103 `field_edit` pinning pattern).
        let query_edit: Option<QueryEditFn<S>> = on_step.as_ref().map(|f| {
            let f = f.clone();
            Rc::new(rune::edit_access(move |s: &mut S| &mut f(s).driver_query)) as QueryEditFn<S>
        });

        // `form.databaseType` — the sidebar's selection branch reads the
        // form machine's current type (captured before the snapshot moves
        // into the body).
        let selected_type = form.database_type.clone();

        // The right-hand form body, built over the same seams the dialog
        // uses (T103). The driver buttons share `on_action`.
        let mut body = ConnectionFormBody::<S>::new(form);
        if let Some(vm_fn) = on_form_vm {
            body = body.on_vm(move |s: &mut S| vm_fn(s));
        }
        if let Some(action_fn) = on_action.clone() {
            body = body.on_action(move |s: &mut S, a| action_fn(s, a));
        }
        let body = body.build(dark, layer);

        // React's dbTypes memo (the availability filter + the query
        // filter), rendered as the sidebar's button list.
        let db_types = step.db_types();

        // The sidebar's scrollable list: `min-h-0 flex-1 overflow-y-auto
        // px-1 pb-2` around the `flex flex-col` of buttons (React's inner
        // wrapper div is a plain flex-col and folds into the scroller).
        let mut list = col()
            .key("onboarding-step-s1-manual-driver-list")
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0 (yields before it scrolls)
            .px(3.0) // px-1
            .pb(6.0) // pb-2
            .scroll(ScrollSpec { stick_bottom: false }); // overflow-y-auto
        if db_types.is_empty() {
            // React's empty branch: `px-3 py-2 text-xs text-fg-muted`.
            list = list.child(
                text(t("newConn.noDriversMatch").to_owned())
                    .key("onboarding-step-s1-manual-driver-empty")
                    .font(TextStyle::new(9.0, 400.0)) // text-xs
                    .px(9.0) // px-3
                    .py(6.0) // py-2
                    .color(k.c.text_muted), // text-fg-muted
            );
        } else {
            for (value, label) in db_types {
                let selected = selected_type == value;
                list = list.child(driver_button(&k, dark, value, label, selected, &on_action));
            }
        }

        // The left driver sidebar: `flex w-[220px] shrink-0 flex-col
        // border-r border-edge bg-surface`.
        let sidebar = col()
            .key("onboarding-step-s1-manual-sidebar")
            .w(SIDEBAR_W) // w-[220px]
            .shrink() // shrink-0
            .min_h(0.0)
            .border_sides([0.0, 0.0, 0.75, 0.0], k.c.border_base) // border-r border-edge
            .fill(k.c.surface_base) // bg-surface
            // The search box: `shrink-0 p-3` around the h-8 mono Input.
            .child(
                col()
                    .key("onboarding-step-s1-manual-driver-search")
                    .shrink() // shrink-0
                    .px(9.0) // p-3
                    .py(9.0)
                    .child(search_field(&step, query_edit.as_ref(), dark)),
            )
            .child(list);

        // The right column: the scrollable form body above the footer.
        let main = col()
            .key("onboarding-step-s1-manual-main")
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0
            .min_w(0.0) // min-w-0
            .child(
                col()
                    .key("onboarding-step-s1-manual-form")
                    .grow(1.0) // flex-1
                    .min_h(0.0) // min-h-0
                    .px(18.0) // p-6
                    .py(18.0)
                    .scroll(ScrollSpec { stick_bottom: false }) // overflow-y-auto
                    .child(body),
            )
            .child(footer(&k, &step, on_test, on_save));

        col()
            .key("onboarding-step-s1-manual") // React data-testid, verbatim
            .w_full()
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0
            // <StepTag label={t('onboarding.s1.stepLabel')} />
            .child(step_tag(&k, t("onboarding.s1.stepLabel")))
            // The heading: mb-2.5 text-[25px] font-bold tracking-tight
            // leading-tight text-fg (the siblings' metrics).
            .child(
                text(t("onboarding.s1.title").to_owned())
                    .key("onboarding-step-s1-manual-title")
                    .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                    .mb(7.5) // mb-2.5
                    .color(k.c.text_base),
            )
            // The subtitle: mb-5 max-w-[56ch] text-[13.5px] leading-[1.65]
            // text-fg-secondary, with the one <b> segment bolded.
            .child(subtitle(&k))
            // The form card: `flex min-h-0 flex-1 overflow-hidden
            // rounded-xl border border-edge` (the NewConnectionDialog
            // layout: sidebar left, form right).
            .child(
                row()
                    .key("onboarding-step-s1-manual-frame")
                    .w_full()
                    .grow(1.0) // flex-1
                    .min_h(0.0) // min-h-0
                    .radius(k.radius.lg) // rounded-xl
                    .clip_rounded(k.radius.lg) // overflow-hidden
                    .border(0.75, k.c.border_base) // border border-edge
                    .child(sidebar)
                    .child(main),
            )
    }
}

/// The sidebar's search field: React's `<Input className="h-8 text-xs
/// font-mono">` — the T103 input chrome (the same React `Input` base) at
/// the h-8 height and xs mono type. Without the step-VM accessor the
/// field is disabled — the honest inert control (the T103 rule).
fn search_field<S: 'static>(
    step: &ManualStepVm,
    edit: Option<&QueryEditFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut field = TextField::new("onboarding-step-s1-manual-driver-search-input", &step.driver_query)
        .height(24.0) // h-8
        .w_full()
        .pad_x(9.0) // the Input base's px-3
        .radius(6.75) // rounded-[9px]
        .fill(k.c.surface_base)
        .border(0.75, k.c.border_base)
        .text_style(TextStyle::new(9.0, 400.0).mono()) // text-xs font-mono
        .placeholder(t("newConn.searchDrivers"));
    match edit {
        Some(edit) => {
            let edit = edit.clone();
            field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
        }
        None => field = field.disabled(true),
    }
    field.build(dark)
}

/// One driver button: `flex items-center gap-2.5 px-3.5 py-2 text-left
/// text-[13px] font-medium` — the selected type wears `bg-accent/10
/// text-fg border-r-2 border-accent`, the others `text-fg-secondary` with
/// the `hover:bg-accent/6` fill hover (the `hover:text-fg` ink half has
/// no counterpart — see the deltas).
fn driver_button<S: 'static>(
    k: &Kit,
    dark: bool,
    value: &str,
    label: &str,
    selected: bool,
    on_action: &Option<FormActionFn<S>>,
) -> El {
    let mut button = row()
        .key(format!("onboard-driver-{value}")) // React data-testid, verbatim
        .w_full()
        .center_y() // items-center
        .gap(7.5) // gap-2.5
        .px(10.5) // px-3.5
        .py(6.0) // py-2
        .hover(k.c.bg_accent.alpha(0.06)) // hover:bg-accent/6
        .child(
            // <DbTypeBadge databaseType={db.value} size={26} /> — the
            // letter tile (the T063 port).
            DbTypeBadge::new(value)
                .size(19.5) // size={26}
                .key(format!("onboard-driver-{value}-badge"))
                .build(dark),
        )
        .child(
            text(label.to_owned())
                .key(format!("onboard-driver-{value}-label"))
                .font(TextStyle::new(9.75, 500.0)) // text-[13px] font-medium
                .color(if selected {
                    k.c.text_base // text-fg
                } else {
                    k.c.text_muted // text-fg-secondary
                }),
        )
        // React: `<button type="button">` — a raw button.
        .role("button")
        .focusable(true);
    if selected {
        // bg-accent/10 + border-r-2 border-accent (the unselected branch
        // has no right border, as in React's class swap).
        button = button
            .fill(k.c.bg_accent.alpha(0.10))
            .border_sides([0.0, 0.0, 1.5, 0.0], k.c.bg_accent);
    }
    // onClick={() => form.handleDatabaseTypeChange(db.value)} — the same
    // action the T103 body's own db-type select dispatches.
    if let Some(cb) = on_action {
        let cb = cb.clone();
        let value = value.to_owned();
        button = button.on_press(move |s: &mut S| cb(s, FormAction::DatabaseType(value.clone())));
    }
    button
}

/// The Test/Save footer: `flex shrink-0 items-center gap-3 border-t
/// border-edge bg-surface px-8 py-3.5` — the secondary Test button (with
/// the flask glyph, busy while testing), the flex-1 mono result line, and
/// the primary Save button locked until a test has passed.
fn footer<S: 'static>(
    k: &Kit,
    step: &ManualStepVm,
    on_test: Option<ActionFn<S>>,
    on_save: Option<ActionFn<S>>,
) -> El {
    // The Test button: secondary (Outline), disabled while testing, its
    // label swapping to the testing pack entry.
    let mut test = Button::new(if step.testing {
        t("newConn.testing").to_owned()
    } else {
        t("onboarding.s1.testBtn").to_owned()
    })
    .variant(ButtonVariant::Outline) // React secondary (T022)
    .control_size(ControlSize::ExtraLarge) // React md (h-9)
    .icon(&ICON_FLASK_CONICAL) // <FlaskConical className="h-3.5 w-3.5" />
    .disabled(step.testing);
    if let Some(on_test) = on_test {
        test = test.on_press(move |s: &mut S| on_test(s));
    }

    // The result line: flex-1 text-xs font-mono — green on ok, red on
    // fail, muted and empty while none (React's `!testResult` arm).
    let (result_text, result_color) = match step.test_result {
        Some(TestOutcome::Ok) => (
            format!("✓ {}", t("onboarding.s1.testSuccess")),
            k.c.success.fg, // text-green
        ),
        Some(TestOutcome::Fail) => (
            format!("✕ {}", t("onboarding.s1.testFail")),
            k.c.danger.fg, // text-red
        ),
        None => (String::new(), k.c.text_muted), // text-fg-muted
    };

    // The Save button: primary (Contrast), the journey's only way forward
    // — locked until a test has passed (`disabled={!tested}`).
    let mut save = Button::new(t("common.save").to_owned())
        .variant(ButtonVariant::Contrast) // React primary (T018)
        .control_size(ControlSize::ExtraLarge) // React md (h-9)
        .disabled(!step.tested);
    if let Some(on_save) = on_save {
        save = save.on_press(move |s: &mut S| on_save(s));
    }

    row()
        .key("onboarding-step-s1-manual-footer")
        .w_full()
        .shrink() // shrink-0
        .center_y() // items-center
        .gap(9.0) // gap-3
        .px(24.0) // px-8
        .py(10.5) // py-3.5
        .border_sides([0.75, 0.0, 0.0, 0.0], k.c.border_base) // border-t border-edge
        .fill(k.c.surface_base) // bg-surface
        .child(test.build(k).key("onboard-test-connection")) // React data-testid, verbatim
        .child(
            text(result_text)
                .key("onboarding-step-s1-manual-test-result")
                .font(TextStyle::new(9.0, 400.0).mono()) // text-xs font-mono
                .grow(1.0) // flex-1
                .color(result_color),
        )
        .child(save.build(k).key("onboard-s1-save")) // React data-testid, verbatim
}

/// The subtitle: the translated string with its one `<b>` segment bolded
/// (a `Span` over the inner byte range — see [`split_b_tags`]).
fn subtitle(k: &Kit) -> El {
    let (plain, bold) = split_b_tags(t("onboarding.s1.subtitle"));
    let spans = match bold {
        Some(range) => vec![Span::new(range.start, range.end).weight(700.0)], // <b>
        None => Vec::new(),
    };
    rich(plain, spans)
        .key("onboarding-step-s1-manual-subtitle")
        .font(TextStyle::new(10.125, 400.0).lh(16.7)) // text-[13.5px] leading-[1.65]
        .max_w(SUBTITLE_MAX_W) // max-w-[56ch]
        .mb(15.0) // mb-5
        .color(k.c.text_muted) // text-fg-secondary
}
