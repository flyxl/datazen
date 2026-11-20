//! `HomeHero`, ported 1:1 from
//! `packages/frontend/src/windows/connection/home/HomeHero.tsx`.
//!
//! React renders a single dynamic summary line (live connection / group /
//! database-type counts) plus a cluster of up to four engine badges, replacing
//! the former three vanity metric cards. The Rune port keeps that structure as
//! a host-owned [`HomeHeroVm`]; every string comes from the locale pack and no
//! interaction is invented (the hero has no buttons in React either).
//!
//! # Consumer / removal stage
//!
//! No production host wiring yet — `views/connections_home.rs` still renders
//! its own inline hero for the A1 landing. The home-wiring task will replace
//! that inline block with [`build_home_hero`] and feed it live store counts;
//! until then every reusable item carries a per-item `#[allow(dead_code)]`
//! with this removal stage, and the attributes can be dropped together.
//!
//! # Keys (preserved from the A1 home tests)
//!
//! | React / A1 key            | meaning                                  |
//! |---------------------------|------------------------------------------|
//! | `connections-home-hero`   | root row                                 |
//! | `connections-home-title`  | `connWin.home.selectConnectionTitle`     |
//! | `connections-home-subtitle`| rich counts line                         |
//! | `home-hero-subtitle`      | React `data-testid` marker (inert)       |
//! | `connections-badge-cluster`| the engine badge cluster                 |
//! | `home-hero-badge-<engine>`| one of the first four engine badges      |

use rune::{col, el, rich, row, text, tok, Color, El, Span, TextStyle};

use crate::i18n::t;
use crate::ui::palette;
use crate::views::engine_label;

/// Host-owned projection of React's `HomeHeroProps`. The host fills the counts
/// from live store data each frame; no fake async or timers live here.
#[allow(dead_code)] // VM surface; consumed by the home wiring and the port tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HomeHeroVm {
    /// React `connectionCount`.
    pub connection_count: usize,
    /// React `groupCount`.
    pub group_count: usize,
    /// Distinct database types across saved connections (React `dbTypes`).
    pub db_types: Vec<String>,
}

#[allow(dead_code)] // VM helper; consumed by counts_line and the tests.
impl HomeHeroVm {
    /// React `dbTypes.length` in the subtitle count.
    pub(crate) fn type_count(&self) -> usize {
        self.db_types.len()
    }

    /// React `dbTypes.slice(0, 4)` for the badge cluster.
    pub(crate) fn badge_types(&self) -> &[String] {
        let end = self.db_types.len().min(4);
        &self.db_types[..end]
    }
}

/// The "N connections · M groups · K database types" line with the leading
/// numbers bold, mirroring React's three `<b>` segments. Returns the content
/// and byte spans for [`rich`]; the separators are plain text (React's `·`).
#[allow(dead_code)] // rendering helper; delete with the host wiring.
pub(crate) fn counts_line(vm: &HomeHeroVm, strong: Color) -> (String, Vec<Span>) {
    let segments = [
        (
            vm.connection_count,
            t("connWin.home.hero.subtitleConnections"),
        ),
        (vm.group_count, t("connWin.home.hero.subtitleGroups")),
        (vm.type_count(), t("connWin.home.hero.subtitleTypes")),
    ];
    let sep = " · ";
    let mut parts: Vec<String> = Vec::with_capacity(segments.len());
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    for (index, (count, word)) in segments.iter().enumerate() {
        if index > 0 {
            cursor += sep.len();
        }
        let head = count.to_string();
        spans.push(Span::new(cursor, cursor + head.len()).bold().color(strong));
        let segment = format!("{head} {word}");
        cursor += segment.len();
        parts.push(segment);
    }
    (parts.join(sep), spans)
}

/// Up to four overlapping engine badges (React's `dbTypes.slice(0, 4)` mapped
/// through `DbTypeBadge` + `getDbLabel`). Rune has no `DbTypeBadge`, so each
/// badge is the same stable hue monogram the A1 home uses; the exact engine
/// label rides on the a11y label where React used the `title` attribute.
#[allow(dead_code)] // rendering helper; delete with the host wiring.
pub(crate) fn badge_cluster(vm: &HomeHeroVm, theme: tok::Theme, dark: bool) -> El {
    let mut cluster = row()
        .key("connections-badge-cluster")
        .items_center()
        .pb(2.0);
    for (index, engine) in vm.badge_types().iter().enumerate() {
        let size = 22.0;
        let initial = engine
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        let mut badge = row()
            .key(format!("home-hero-badge-{engine}"))
            .label(engine_label(engine))
            .size(size, size)
            .radius(size * 0.28)
            .center()
            .fill(palette::engine_badge_fill(dark, engine_hue(engine)))
            .child(
                text(initial)
                    .font(TextStyle::new(size * 0.42, 700.0))
                    .color(palette::badge_ink(dark)),
            );
        if index > 0 {
            badge = badge.ml(-6.0);
        }
        cluster = cluster.child(badge.border(2.0, theme.bg));
    }
    cluster
}

/// Build the hero. Keys match the A1 home so the existing probes keep working.
#[allow(dead_code)] // consumer: the connections-home host that mounts this view (not landed); delete when it replaces the inline hero.
pub(crate) fn build_home_hero(vm: &HomeHeroVm, theme: tok::Theme, dark: bool) -> El {
    let (line, spans) = counts_line(vm, theme.text);
    let mut hero = row()
        .key("connections-home-hero")
        .w_full()
        .items_end()
        .gap(16.0)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(
                    text(t("connWin.home.selectConnectionTitle"))
                        .key("connections-home-title")
                        .font(TextStyle::new(20.0, 700.0))
                        .color(theme.text),
                )
                .child(
                    rich(line, spans)
                        .key("connections-home-subtitle")
                        .font(TextStyle::new(13.0, 400.0))
                        .color(theme.text_secondary),
                )
                .child(react_key("home-hero-subtitle")),
        );
    if !vm.db_types.is_empty() {
        hero = hero.child(badge_cluster(vm, theme, dark));
    }
    hero
}

/// A zero-size, inert marker for a React `data-testid` with no Rune element.
#[allow(dead_code)] // key parity marker; delete with the A1 key migration.
fn react_key(key: &str) -> El {
    el().key(key).a11y_hidden()
}

/// Stable accent hue per engine, shared with the A1 home badge convention.
#[allow(dead_code)] // rendering helper; delete with the host wiring.
fn engine_hue(engine: &str) -> f32 {
    match engine.to_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => 256.0,
        "sqlite" | "sqlite3" => 152.0,
        "mysql" | "mariadb" => 58.0,
        "mongodb" | "mongo" => 152.0,
        "redis" => 25.0,
        "sqlserver" | "mssql" => 220.0,
        "clickhouse" => 300.0,
        "duckdb" => 80.0,
        _ => {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in engine.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            (hash % 360) as f32
        }
    }
}
