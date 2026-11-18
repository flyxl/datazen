//! Port of `packages/frontend/src/components/schema/SchemaDiffPanel.tsx`.
//!
//! A pure display component: one root column of optional sections, each section
//! a header plus mono rows (or changed-column cards). No interaction, no state.
//! React's `text-*` classes map to kit semantic colors (`success`/`danger`/
//! `warning` for headers, `text_muted`/`text_faint` for secondary copy); every
//! color comes from `kit(dark)`, so no raw color literal is needed here.
//!
//! This port has no production consumer yet; it will be wired by the
//! schema-diff view track. Reusable API items are kept with per-item
//! `#[allow(dead_code)]` (not file-level) until wired, like `components/history`.
//!
//! ## Keys
//!
//! - `schema-diff-panel`: root column
//! - `schema-diff-target-only-detail`: danger-tinted target-only box (React `data-testid`)
//! - `schema-diff-section-{missing,extra,changed,check-missing,check-extra,table-options}`
//! - `schema-diff-header-{...}`: section header text
//! - `schema-diff-{missing,extra}-row-{name}`: `+`/`-` column rows
//! - `schema-diff-changed-{name}` and its `-name`/`-source`/`-target`/`-changes` children
//! - `schema-diff-check-{missing,extra}-row-{name}`: CHECK constraint rows
//! - `schema-diff-option-row-{change}`: `~ change: target -> source` row
//! - `schema-diff-identical`: the fallback line when nothing differs

use rune::{col, text, Color, El};

use crate::i18n::t;
use crate::ui::kit::kit;

// ---------------------------------------------------------------------------
// View models (mirror the React `TableSchemaDiff` / nested types)
// ---------------------------------------------------------------------------

/// `ColumnDiffEntry` from `packages/frontend/src/types/index.ts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct ColumnDiffVm {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

#[allow(dead_code)] // reusable port; wired by the schema-diff view track
impl ColumnDiffVm {
    pub(crate) fn new(
        name: impl Into<String>,
        data_type: impl Into<String>,
        nullable: bool,
        is_primary_key: bool,
    ) -> Self {
        Self {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            is_primary_key,
        }
    }
}

/// `ChangedColumnDiff` — one changed-column card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct ChangedColumnDiffVm {
    pub name: String,
    pub source: ColumnDiffVm,
    pub target: ColumnDiffVm,
    pub changes: Vec<String>,
}

/// `CheckConstraintDiffEntry`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct CheckConstraintDiffVm {
    pub name: String,
    pub expression: String,
}

/// `TableOptionsSnapshot` (driver-sdk): the three option keys React indexes by
/// `tableOptions.source[change]` / `tableOptions.target[change]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct TableOptionsSnapshotVm {
    pub comment: Option<String>,
    pub engine: Option<String>,
    pub charset: Option<String>,
}

impl TableOptionsSnapshotVm {
    /// React's `snapshot[change as keyof typeof snapshot] ?? ''`. Unknown keys
    /// (and null/undefined values) become the empty string, as in `String(x ?? '')`.
    fn value(&self, change: &str) -> String {
        let value = match change {
            "comment" => self.comment.as_deref(),
            "engine" => self.engine.as_deref(),
            "charset" => self.charset.as_deref(),
            _ => None,
        };
        value.unwrap_or("").to_string()
    }
}

/// `TableOptionsDiff`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct TableOptionsDiffVm {
    pub source: TableOptionsSnapshotVm,
    pub target: TableOptionsSnapshotVm,
    pub changes: Vec<String>,
}

/// `TableSchemaDiff` with the legacy aliases the React component falls back to:
/// `missingOnTarget ?? added` and `extraOnTarget ?? removed`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct TableSchemaDiffVm {
    pub table: String,
    /// React `targetOnly`: the table exists only on the target.
    pub target_only: bool,
    /// Preferred source of the `+` column rows.
    pub missing_on_target: Option<Vec<ColumnDiffVm>>,
    /// Preferred source of the `-` column rows.
    pub extra_on_target: Option<Vec<ColumnDiffVm>>,
    /// Legacy alias of `missing_on_target`.
    pub added: Vec<ColumnDiffVm>,
    /// Legacy alias of `extra_on_target`.
    pub removed: Vec<ColumnDiffVm>,
    pub changed: Vec<ChangedColumnDiffVm>,
    pub missing_check_constraints: Option<Vec<CheckConstraintDiffVm>>,
    pub extra_check_constraints: Option<Vec<CheckConstraintDiffVm>>,
    pub table_options: Option<TableOptionsDiffVm>,
}

#[allow(dead_code)] // reusable port; wired by the schema-diff view track
impl TableSchemaDiffVm {
    /// `diff.missingOnTarget ?? diff.added`.
    pub(crate) fn missing(&self) -> &[ColumnDiffVm] {
        self.missing_on_target.as_deref().unwrap_or(&self.added)
    }

    /// `diff.extraOnTarget ?? diff.removed`.
    pub(crate) fn extra(&self) -> &[ColumnDiffVm] {
        self.extra_on_target.as_deref().unwrap_or(&self.removed)
    }

    /// `diff.missingCheckConstraints ?? []`.
    pub(crate) fn missing_checks(&self) -> &[CheckConstraintDiffVm] {
        self.missing_check_constraints.as_deref().unwrap_or_default()
    }

    /// `diff.extraCheckConstraints ?? []`.
    pub(crate) fn extra_checks(&self) -> &[CheckConstraintDiffVm] {
        self.extra_check_constraints.as_deref().unwrap_or_default()
    }

    /// The React `identical` expression: no target-only flag, all arrays empty
    /// and no table-options object at all (`!tableOptions`, not "empty changes").
    pub(crate) fn is_identical(&self) -> bool {
        !self.target_only
            && self.missing().is_empty()
            && self.extra().is_empty()
            && self.changed.is_empty()
            && self.missing_checks().is_empty()
            && self.extra_checks().is_empty()
            && self.table_options.is_none()
    }
}

// ---------------------------------------------------------------------------
// Shared text helpers — the React strings are assembled in exactly one place
// so the view and the clipboard summary cannot drift apart.
// ---------------------------------------------------------------------------

/// `, NOT NULL` / `, PK` suffix used by the panel rows (React lines 35-37).
fn column_suffix(col: &ColumnDiffVm) -> String {
    let mut suffix = String::new();
    if !col.nullable {
        suffix.push_str(", NOT NULL");
    }
    if col.is_primary_key {
        suffix.push_str(", PK");
    }
    suffix
}

/// Secondary text (React `text-fg-secondary`): mono, muted, wraps instead of
/// overflowing at the minimum window width.
fn mono_row(k: &rune_kit::Kit, key: &str, content: String) -> El {
    text(content)
        .key(key)
        .font(k.ty.mono)
        .color(k.c.text_muted)
        .w_full()
        .min_w(0.0)
        .wrap_anywhere()
}

fn header(k: &rune_kit::Kit, key: &str, label: &str, color: Color) -> El {
    text(label)
        .key(key)
        .font(k.ty.text_xsmall_medium)
        .color(color)
        .w_full()
        .min_w(0.0)
        .wrap_anywhere()
}

fn section(key: &str, head: El, rows: Vec<El>) -> El {
    let mut el = col()
        .key(key)
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .child(head);
    for row in rows {
        el = el.child(row);
    }
    el
}

// ---------------------------------------------------------------------------
// Plain-text clipboard summary (React `formatSchemaDiffText`)
// ---------------------------------------------------------------------------

/// Plain-text summary for clipboard / ALTER hints.
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) fn format_schema_diff_text(diff: &TableSchemaDiffVm) -> String {
    let mut lines: Vec<String> = vec![format!("-- Schema diff: {}", diff.table)];

    for col in diff.missing() {
        let not_null = if col.nullable { "" } else { " NOT NULL" };
        lines.push(format!("+ {} {}{}", col.name, col.data_type, not_null));
    }
    for col in diff.extra() {
        lines.push(format!("- {} {}", col.name, col.data_type));
    }
    for col in &diff.changed {
        lines.push(format!(
            "~ {}: {} -> {} ({})",
            col.name,
            col.target.data_type,
            col.source.data_type,
            col.changes.join(", ")
        ));
    }
    for constraint in diff.missing_checks() {
        lines.push(format!(
            "+ {}: CHECK ({})",
            constraint.name, constraint.expression
        ));
    }
    for constraint in diff.extra_checks() {
        lines.push(format!(
            "- {}: CHECK ({})",
            constraint.name, constraint.expression
        ));
    }
    if let Some(options) = &diff.table_options {
        for change in &options.changes {
            lines.push(format!(
                "~ table {}: {} -> {}",
                change,
                options.target.value(change),
                options.source.value(change)
            ));
        }
    }
    if lines.len() == 1 {
        lines.push("(identical)".to_string());
    }
    lines.join("\n")
}

/// CamelCase alias kept for parity with the React export's name. The Rust
/// entry point is [`format_schema_diff_text`].
#[allow(non_snake_case, dead_code)] // React export name; wired by the view track
pub(crate) fn formatSchemaDiffText(diff: &TableSchemaDiffVm) -> String {
    format_schema_diff_text(diff)
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// `SchemaDiffPanel({ diff })` — a stateless builder; `build(dark)` returns the
/// root element. `space-y-4` (16pt) separates sections.
#[allow(dead_code)] // reusable port; wired by the schema-diff view track
pub(crate) struct SchemaDiffPanel {
    diff: TableSchemaDiffVm,
}

#[allow(dead_code)] // reusable port; wired by the schema-diff view track
impl SchemaDiffPanel {
    pub(crate) fn new(diff: TableSchemaDiffVm) -> Self {
        Self { diff }
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let diff = self.diff;
        let mut root = col()
            .key("schema-diff-panel")
            .w_full()
            .min_w(0.0)
            .gap(16.0);

        // `targetOnly` → danger-tinted box with the React data-testid key.
        if diff.target_only {
            root = root.child(
                text(t("schemaDiff.targetOnlyDetail"))
                    .key("schema-diff-target-only-detail")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .w_full()
                    .min_w(0.0)
                    .wrap_anywhere()
                    .radius(4.0)
                    .border(1.0, k.alpha(k.c.danger.fg, 0.3))
                    .fill(k.alpha(k.c.danger.fg, 0.1))
                    .padding(12.0),
            );
        }

        // Missing on target (ADD) → success header, `+ name (type[-suffixes])`.
        let missing = diff.missing();
        if !missing.is_empty() {
            let rows = missing
                .iter()
                .map(|col| {
                    mono_row(
                        k,
                        &format!("schema-diff-missing-row-{}", col.name),
                        format!(
                            "+ {} ({}{})",
                            col.name,
                            col.data_type,
                            column_suffix(col)
                        ),
                    )
                })
                .collect();
            root = root.child(section(
                "schema-diff-section-missing",
                header(
                    k,
                    "schema-diff-header-missing",
                    t("schemaDiff.missingOnTarget"),
                    k.c.success.fg,
                ),
                rows,
            ));
        }

        // Extra on target (DROP) → danger header, `- name (type[-suffixes])`.
        let extra = diff.extra();
        if !extra.is_empty() {
            let rows = extra
                .iter()
                .map(|col| {
                    mono_row(
                        k,
                        &format!("schema-diff-extra-row-{}", col.name),
                        format!(
                            "- {} ({}{})",
                            col.name,
                            col.data_type,
                            column_suffix(col)
                        ),
                    )
                })
                .collect();
            root = root.child(section(
                "schema-diff-section-extra",
                header(
                    k,
                    "schema-diff-header-extra",
                    t("schemaDiff.extraOnTarget"),
                    k.c.danger.fg,
                ),
                rows,
            ));
        }

        // Changed columns → warning header, one bordered card per column.
        if !diff.changed.is_empty() {
            let mut rows = Vec::new();
            for col in &diff.changed {
                let base = format!("schema-diff-changed-{}", col.name);
                let card = col_block(k, &base, col);
                rows.push(card);
            }
            root = root.child(section(
                "schema-diff-section-changed",
                header(
                    k,
                    "schema-diff-header-changed",
                    t("schemaDiff.colChanged"),
                    k.c.warning.fg,
                ),
                rows,
            ));
        }

        // Missing CHECK constraints → success header, `+ name: CHECK (expr)`.
        let missing_checks = diff.missing_checks();
        if !missing_checks.is_empty() {
            let rows = missing_checks
                .iter()
                .map(|c| {
                    mono_row(
                        k,
                        &format!("schema-diff-check-missing-row-{}", c.name),
                        format!("+ {}: CHECK ({})", c.name, c.expression),
                    )
                })
                .collect();
            root = root.child(section(
                "schema-diff-section-check-missing",
                header(
                    k,
                    "schema-diff-header-check-missing",
                    t("schemaDiff.checkMissing"),
                    k.c.success.fg,
                ),
                rows,
            ));
        }

        // Extra CHECK constraints → danger header, `- name: CHECK (expr)`.
        let extra_checks = diff.extra_checks();
        if !extra_checks.is_empty() {
            let rows = extra_checks
                .iter()
                .map(|c| {
                    mono_row(
                        k,
                        &format!("schema-diff-check-extra-row-{}", c.name),
                        format!("- {}: CHECK ({})", c.name, c.expression),
                    )
                })
                .collect();
            root = root.child(section(
                "schema-diff-section-check-extra",
                header(
                    k,
                    "schema-diff-header-check-extra",
                    t("schemaDiff.checkExtra"),
                    k.c.danger.fg,
                ),
                rows,
            ));
        }

        // Table options → warning header. React renders `target -> source`
        // (the direction is deliberate: target value first).
        if let Some(options) = &diff.table_options {
            let rows = options
                .changes
                .iter()
                .map(|change| {
                    mono_row(
                        k,
                        &format!("schema-diff-option-row-{change}"),
                        format!(
                            "~ {}: {} -> {}",
                            change,
                            options.target.value(change),
                            options.source.value(change)
                        ),
                    )
                })
                .collect();
            root = root.child(section(
                "schema-diff-section-table-options",
                header(
                    k,
                    "schema-diff-header-table-options",
                    t("schemaDiff.tableOptions"),
                    k.c.warning.fg,
                ),
                rows,
            ));
        }

        if diff.is_identical() {
            root = root.child(
                text(t("schemaDiff.schemaIdentical"))
                    .key("schema-diff-identical")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .w_full()
                    .min_w(0.0)
                    .wrap_anywhere(),
            );
        }

        root
    }
}

/// One changed-column card: name, source line, target line, changes line.
fn col_block(k: &rune_kit::Kit, base: &str, change: &ChangedColumnDiffVm) -> El {
    let name = text(&change.name)
        .key(format!("{base}-name"))
        .font(k.ty.text_small_medium)
        .color(k.c.text_base)
        .w_full()
        .min_w(0.0)
        .wrap_anywhere();

    let source = text(format!(
        "{}: {}{}",
        t("schemaDiff.source"),
        change.source.data_type,
        column_suffix(&change.source)
    ))
    .key(format!("{base}-source"))
    .font(k.ty.mono)
    .color(k.c.text_muted)
    .w_full()
    .min_w(0.0)
    .wrap_anywhere();

    let target = text(format!(
        "{}: {}{}",
        t("schemaDiff.target"),
        change.target.data_type,
        column_suffix(&change.target)
    ))
    .key(format!("{base}-target"))
    .font(k.ty.mono)
    .color(k.c.text_muted)
    .w_full()
    .min_w(0.0)
    .wrap_anywhere();

    let changes = text(change.changes.join(", "))
        .key(format!("{base}-changes"))
        .font(k.ty.mono)
        .color(k.c.text_faint)
        .w_full()
        .min_w(0.0)
        .wrap_anywhere();

    col().key(base)
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .radius(4.0)
        .border(1.0, k.c.border_base)
        .fill(k.c.layer_01)
        .padding(8.0)
        .child(name)
        .child(source)
        .child(target)
        .child(changes)
}
