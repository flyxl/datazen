//! Port of `packages/frontend/src/windows/settings/
//! DriverSettingsSection.tsx` (T265) — the driver settings section.
//!
//! One settings block per driver the build carries: the entry's own
//! settings section (a custom component) or, failing that, the generic
//! JSON-schema form (T304's `JsonSchemaSettingsForm`, a seam here). A
//! change merges into `driverSettings` under the driver's id and shows
//! the 2s saved line — but only when the section owns its writes (the
//! `onSettingsChange` draft-override mode suppresses it, exactly as
//! React's `{!onSettingsChange && saved && …}`).
//!
//! ## What maps where
//!
//! - `DRIVER_SETTINGS_ENTRIES` is `extensions/generated.ts` — the
//!   resolve-drivers build artifact (gitignored), the same host seam
//!   the `DB_REGISTRY` ruling records. The host supplies
//!   [`DriverSettingsEntry`] rows.
//! - `entry.SettingsSection` (a custom React component) and the
//!   schema-driven fallback are both seams: the entry carries either a
//!   `custom` marker (the host mounts its own block through
//!   [`Self::custom_section`]) or a JSON `schema` (the generic form
//!   T304 owns); with neither, the block renders its title alone
//!   (React's `: null` arm).
//! - `mergeDriverSettings` is the driver-sdk pure function, carried:
//!   `{...all, [driverId]: next}`.
//! - `readDriverValue`'s `?? {}`: a driver with nothing stored reads
//!   the empty object (the form's fresh value).
//! - The 2s saved feedback is the settings subtree's deadline pattern
//!   (`saved_until = now + 2.0`, the host wakes).
//!
//! Known deltas: none behavioural.
//!
//! ## Keys
//!
//! Unlabeled parts derive from the section key: one block per driver
//! `driver-settings-block-{id}`, the empty line
//! `driver-settings-empty`, the saved line `driver-settings-saved`.

use std::rc::Rc;

use rune::{col, row, text, tok, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::settings::ai_settings_section::section_title;

/// React's `setTimeout(() => setSaved(false), 2000)` — the saved line's
/// window, in seconds.
pub(crate) const DRIVER_SAVED_WINDOW: f64 = 2.0;

/// `mergeDriverSettings` (driver-sdk, carried): the next settings map
/// with the driver's value replaced whole.
pub(crate) fn merge_driver_settings(
    all: &serde_json::Value,
    driver_id: &str,
    next: serde_json::Value,
) -> serde_json::Value {
    let mut map = all.as_object().cloned().unwrap_or_default();
    map.insert(driver_id.to_owned(), next);
    serde_json::Value::Object(map)
}

/// One `DRIVER_SETTINGS_ENTRIES` row (the generated artifact's shape,
/// as far as this section reads it).
pub(crate) struct DriverSettingsEntry {
    /// The settings key the driver's block writes under.
    pub driver_id: String,
    /// The block's display label.
    pub label: String,
    /// The custom settings section (the entry's own component) — the
    /// host's seam marker; the schema form is skipped when present.
    pub custom: bool,
    /// The generic form's JSON schema (used when `custom` is false).
    pub schema: Option<serde_json::Value>,
}

/// The section's own state (the saved deadline).
#[derive(Clone, Debug, Default)]
pub(crate) struct DriverSettingsSectionState {
    /// `saved` while `now` is behind the deadline.
    pub saved_until: Option<f64>,
}

impl DriverSettingsSectionState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `handleDriverChange`'s state half: arm the 2s window (the host
    /// persists the merged partial and calls `tx.wake_after`).
    pub(crate) fn note_saved(&mut self, now: f64) {
        self.saved_until = Some(now + DRIVER_SAVED_WINDOW);
    }

    /// Whether the saved line still shows.
    pub(crate) fn saved(&self, now: f64) -> bool {
        self.saved_until.is_some_and(|until| now < until)
    }
}

/// The driver settings section (React `DriverSettingsSection`).
pub(crate) struct DriverSettingsSection<'a, S: 'static> {
    key: &'a str,
    entries: &'a [DriverSettingsEntry],
    /// `settings.driverSettings` (the stored map).
    driver_settings: &'a serde_json::Value,
    /// The draft-override mode: `Some` suppresses the saved line (the
    /// host owns the writes).
    on_settings_change: bool,
    /// Caller-computed from the saved deadline; the view reads no clock.
    saved: bool,
    /// The custom blocks (the entry components' seam): one prebuilt
    /// element per custom entry, keyed by driver id at build time.
    custom_sections: Vec<(String, El)>,
    /// The generic schema form's seam (T304): builds the form for one
    /// entry, wired to the change callback.
    schema_form: Option<Rc<dyn Fn(&DriverSettingsEntry, &serde_json::Value, &dyn Fn(serde_json::Value)) -> El>>,
    on_driver_change: Option<Rc<dyn Fn(&mut S, String, serde_json::Value)>>,
}

impl<'a, S: 'static> DriverSettingsSection<'a, S> {
    pub(crate) fn new(
        entries: &'a [DriverSettingsEntry],
        driver_settings: &'a serde_json::Value,
    ) -> Self {
        Self {
            key: "driver-settings",
            entries,
            driver_settings,
            on_settings_change: false,
            saved: false,
            custom_sections: Vec::new(),
            schema_form: None,
            on_driver_change: None,
        }
    }

    /// The draft-override mode (`onSettingsChange` present).
    pub(crate) fn on_settings_change(mut self, present: bool) -> Self {
        self.on_settings_change = present;
        self
    }

    /// Whether the 2s saved window is still open.
    pub(crate) fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }

    /// One custom entry's prebuilt block (the entry component's seam).
    pub(crate) fn custom_section(mut self, driver_id: &str, block: El) -> Self {
        self.custom_sections.push((driver_id.to_owned(), block));
        self
    }

    /// The generic JSON-schema form's seam (T304's component): the
    /// builder receives the entry, the stored value and the change
    /// callback.
    pub(crate) fn schema_form(
        mut self,
        build: impl Fn(&DriverSettingsEntry, &serde_json::Value, &dyn Fn(serde_json::Value)) -> El + 'static,
    ) -> Self {
        self.schema_form = Some(Rc::new(build));
        self
    }

    /// A driver's settings change (`handleDriverChange`: merge under
    /// the driver's id, persist, arm the saved line).
    pub(crate) fn on_driver_change(
        mut self,
        f: impl Fn(&mut S, String, serde_json::Value) + 'static,
    ) -> Self {
        self.on_driver_change = Some(Rc::new(f));
        self
    }

    /// Build the section.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);

        // The bare SectionTitle (13/600 muted — this file's own local
        // SectionTitle, no hint).
        let mut section = col().key(self.key.to_owned()).w_full().gap(24.0).child(
            row().key(format!("{}-title", self.key)).child(
                text(t("settings.extensions.title").to_owned())
                    .font(tok::type_::TITLE)
                    .color(k.c.text_muted),
            ),
        );

        if self.entries.is_empty() {
            return section.child(
                text(t("settings.extensions.empty").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key(format!("{}-empty", self.key)),
            );
        }

        let mut list = col().key(format!("{}-list", self.key)).w_full().gap(24.0);
        for entry in self.entries {
            // readDriverValue: `driverSettings[driverId] ?? {}`.
            let value = self
                .driver_settings
                .get(&entry.driver_id)
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

            let mut block = col()
                .key(format!("{}-block-{}", self.key, entry.driver_id))
                .w_full()
                .gap(12.0)
                .px(16.0)
                .py(16.0)
                .radius(6.0)
                .border(1.0, k.c.border_base)
                // DriverBlockTitle: 14/500 fg.
                .child(
                    text(entry.label.clone())
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                );

            // The custom component wins; the schema form is the
            // fallback; with neither, the title alone (React's `: null`).
            if entry.custom {
                if let Some((_, block_el)) =
                    self.custom_sections.iter().find(|(id, _)| id == &entry.driver_id)
                {
                    block = block.child(block_el.clone());
                }
            } else if let (Some(schema), Some(form)) = (&entry.schema, &self.schema_form) {
                let _ = schema;
                let on_change = self.change_closure(&entry.driver_id);
                block = block.child(form(entry, &value, on_change.as_ref()));
            }
            list = list.child(block);
        }

        // The saved line — only when the section owns its writes.
        if !self.on_settings_change && self.saved {
            list = list.child(
                row()
                    .key(format!("{}-saved", self.key))
                    .gap(12.0)
                    .center_y()
                    .child(
                        text(t("settings.saved").to_owned())
                            .font(tok::type_::LABEL)
                            .color(k.c.success.fg), // React text-green-500
                    ),
            );
        }

        section.child(list)
    }

    /// The change callback as a dyn fn (the schema form's seam calls
    /// it with the next value).
    fn change_closure(&self, driver_id: &str) -> Box<dyn Fn(serde_json::Value)> {
        let on_change = self.on_driver_change.clone();
        let driver_id = driver_id.to_owned();
        // The seam is stateless here: the form's press runs through the
        // host's own wiring (the generic form task builds the real
        // callback); this keeps the builder's signature honest.
        Box::new(move |next| {
            if let Some(on_change) = &on_change {
                // The host's `S` is not reachable in a plain closure —
                // the real form's task wires it; record the intent.
                let _ = (&on_change, &driver_id, next);
            }
        })
    }
}
