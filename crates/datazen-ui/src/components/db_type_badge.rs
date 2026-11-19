//! T063: `components/DbTypeBadge.tsx` ported to Rune.
//!
//! React resolves `db.<type>` through the theme-pack icon resolver and
//! renders, in order: the type's own bundled SVG (`<img>`), a composite of
//! the parent type's SVG with the own short label in the corner (for
//! protocol-reuse types whose own SVG is missing from the build), or a
//! letter tile — the driver's short label in bold white on its `iconBg`
//! tone — when neither URL exists. The resolver's placeholder for a db id
//! is `getDbIcon` (see `lib/bootstrapIconResolver.ts`), so the tile and
//! the corner label both come from the driver meta; an unknown type falls
//! back to `DB` on the neutral tone, exactly as `getDbIcon` does.
//!
//! The production consumers are the connection views (NewConnectionDialog,
//! ConnectionCardList, HomeHero, NavigatorTreeRow, BackupWindow, …); until
//! the wiring track lands them, the component tests are the only callers,
//! so reusable API items carry per-item `#[allow(dead_code)]` (not a
//! module-wide blanket).
//!
//! ## Deliberate deviations
//!
//! * **The badge is always the letter tile.** Rune has no theme-pack icon
//!   loading and this migration renders no remote SVG (`<img>`), so the
//!   URL and composite branches can never render — the same deviation the
//!   Backup window already records for its inline badge. The decision
//!   tree is kept as pure, tested functions ([`db_icon_parent`] for the
//!   parent map, [`badge_font_size`] / [`corner_font_size`] for the
//!   sizing) so the wiring track can revive the composite branch if SVG
//!   loading ever lands; the tile itself is [`db_badge_meta`], which is
//!   also the canonical home of the table the Backup window used to keep
//!   inline (it imports it from here now).
//! * **No `resolver` prop.** React accepts a custom `IconResolver` (its
//!   tests inject one); Rune has no resolver to inject, and the tile
//!   always uses the driver meta — the same answer the bootstrapped
//!   resolver gives. A theme-pack override therefore cannot change the
//!   badge, where React would swap in the pack's SVG.
//! * **No theme-pack-changed listener.** React re-renders on
//!   `datazen:theme-pack-changed` because the resolved URL can change;
//!   the tile's colours come from the kit, not the pack, so there is
//!   nothing to re-subscribe to.
//! * **No `className` passthrough.** React merges an optional class onto
//!   the badge; Rune callers compose layout around the element instead.
//! * **The corner label is not rendered.** It only exists on the
//!   composite branch (parent SVG + own label), which cannot render, so
//!   [`corner_font_size`] is currently exercised by tests only — it
//!   documents the sizing rule for the branch that would use it.

use rune::{row, text, Color, El, TextStyle};

use crate::ui::badge::BadgeTone;
use crate::ui::kit::kit;

/// React `DRIVER_ICON_PARENT` (`scripts/resolve-drivers.mjs:295`):
/// protocol-reuse db types → the parent db type whose SVG the composite
/// badge shows when the own SVG is missing from the build. Every type
/// listed here ships its own SVG in the full build
/// (`crates/drivers/*/ui/icons/`), so the composite only fires in a
/// reduced-driver build; in Rune it never fires (see the module docs).
#[allow(dead_code)] // decision-tree record; revived if SVG loading lands
pub(crate) fn db_icon_parent(db_type: &str) -> Option<&'static str> {
    match db_type {
        "questdb" => Some("postgresql"),
        "cloudberry" => Some("postgresql"),
        "doris" => Some("mysql"),
        "starrocks" => Some("mysql"),
        "manticore" => Some("mysql"),
        "ob_oracle" => Some("mysql"),
        _ => None,
    }
}

/// React `getDbIcon` (`lib/databaseTypes.ts`): the driver's short label
/// and `iconBg` tone, from the shipped driver metas
/// (`crates/drivers/*/ui/meta.ts`); an unknown type falls back to `DB`
/// on the neutral tone. Canonical home of the table the Backup window
/// used to keep inline.
pub(crate) fn db_badge_meta(db_type: &str) -> (&'static str, BadgeTone) {
    match db_type {
        "mysql" => ("My", BadgeTone::Warning),
        "mariadb" => ("Ma", BadgeTone::Warning),
        "doris" => ("Do", BadgeTone::Warning),
        "starrocks" => ("Sr", BadgeTone::Warning),
        "manticore" => ("Mc", BadgeTone::Warning),
        "ob_oracle" => ("Ob", BadgeTone::Warning),
        "postgresql" => ("Pg", BadgeTone::Accent),
        "questdb" => ("Qd", BadgeTone::Accent),
        "cloudberry" => ("Cb", BadgeTone::Accent),
        "sqlite" => ("Lt", BadgeTone::Success),
        "rqlite" => ("Rq", BadgeTone::Accent),
        "turso" => ("Tu", BadgeTone::Success),
        "sqlserver" => ("Ss", BadgeTone::Danger),
        "clickhouse" => ("Ch", BadgeTone::Warning),
        "duckdb" => ("Dk", BadgeTone::Accent),
        "redis" => ("Rd", BadgeTone::Danger),
        "mongodb" => ("Mb", BadgeTone::Success),
        "elasticsearch" => ("Es", BadgeTone::Warning),
        "hbase" => ("Hb", BadgeTone::Warning),
        "influxdb" => ("If", BadgeTone::Warning),
        "vector" => ("Vc", BadgeTone::Success),
        "victoriametrics" => ("Vm", BadgeTone::Accent),
        _ => ("DB", BadgeTone::Neutral),
    }
}

/// The tile fill behind a tone: the `iconBg` class projected onto kit
/// tokens — warning / accent / success / danger paint with their
/// foreground token, neutral with faint text (the same projection the
/// Backup window's inline badge used).
pub(crate) fn badge_tone_fill(tone: BadgeTone, dark: bool) -> Color {
    let k = kit(dark);
    match tone {
        BadgeTone::Neutral => k.c.text_faint,
        BadgeTone::Success => k.c.success.fg,
        BadgeTone::Warning => k.c.warning.fg,
        BadgeTone::Danger => k.c.danger.fg,
        BadgeTone::Accent => k.c.bg_accent,
    }
}

/// React `badgeFontSize`: 9pt at size 20 or below, 10pt at the default
/// 24, 11pt above that.
pub(crate) fn badge_font_size(size: f32) -> f32 {
    if size <= 20.0 {
        9.0
    } else if size <= 24.0 {
        10.0
    } else {
        11.0
    }
}

/// React `cornerFontSize`: `size * 0.38`, rounded, clamped to 8–11pt.
/// Only the composite branch uses it (see the module docs).
#[allow(dead_code)] // composite branch only; exercised by tests
pub(crate) fn corner_font_size(size: f32) -> f32 {
    (size * 0.38).round().clamp(8.0, 11.0)
}

/// The letter-tile badge: a `size` square, `rounded-lg`, the short label
/// in bold white on the tone fill, `aria-hidden` like React's span.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by the connection views
pub(crate) struct DbTypeBadge {
    key: String,
    database_type: String,
    size: f32,
}

#[allow(dead_code)] // reusable component; wired by the connection views
impl DbTypeBadge {
    /// React `<DbTypeBadge databaseType={…} />`; `size` defaults to 24,
    /// the compact badge React uses everywhere.
    pub(crate) fn new(database_type: impl Into<String>) -> Self {
        Self {
            key: "db-type-badge".to_string(),
            database_type: database_type.into(),
            size: 24.0,
        }
    }

    /// Override the key prefix (two badges under one parent need
    /// distinct keys; the default is one fixed string).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// React's `size` prop: pixel width/height of the square.
    pub(crate) fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Builds the tile. The font follows [`badge_font_size`]; the label
    /// and tone follow [`db_badge_meta`].
    pub(crate) fn build(self, dark: bool) -> El {
        let (label, tone) = db_badge_meta(&self.database_type);
        let font = badge_font_size(self.size);
        row()
            .key(self.key)
            .w(self.size)
            .h(self.size)
            .center()
            .radius(8.0)
            .fill(badge_tone_fill(tone, dark))
            .a11y_hidden()
            .child(
                text(label.to_owned())
                    .font(TextStyle::new(font, 700.0).lh(font + 2.0))
                    .color(Color::WHITE),
            )
    }
}
