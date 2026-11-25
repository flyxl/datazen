//! Port of `packages/frontend/src/components/chart/ChartEmptyState.tsx` (Wave 2, T059).
//!
//! The React component is a centered pane `ChartView` mounts when there is
//! nothing to draw: one dimmed `BarChart3` glyph (`h-12 w-12 opacity-30`) and
//! the translated reason line. All three reasons come from the zh-CN chart
//! pack through `crate::i18n::t`, keyed exactly as React's `messages` record
//! (no invented copy).
//!
//! Rune has no `useI18n`; `t()` reads the same generated table. The glyph is
//! the registry's `ICON_CHART_COLUMN`: `lucide-react` 0.487 (the version
//! `pnpm-lock.yaml` pins) aliases `BarChart3` to the renamed `chart-column`
//! artwork, so that artwork — not the older `bar-chart-3` already registered
//! for the workflow panel — is what the React page paints here. React's
//! `opacity-30` becomes a 0.3 alpha on the muted tint (an alpha on the icon
//! colour, not an element opacity, so the pane stays in the a11y tree).
//!
//! ## Keys
//!
//! | part    | key                     |
//! |---------|-------------------------|
//! | root    | `chart-empty-state`     |
//! | icon    | `chart-empty-state-icon`|
//! | message | `chart-empty-state-text`|
//!
//! All colours and fonts come from `kit(dark)`; no raw `oklch`, no recharts.

use rune::{col, icon, text, El};

use crate::i18n::t;
use crate::icons::ICON_CHART_COLUMN;
use crate::ui::kit::kit;

/// Root pane, React's `flex flex-1 flex-col items-center justify-center`.
pub const EMPTY_KEY: &str = "chart-empty-state";
/// The dimmed chart glyph.
pub const EMPTY_ICON_KEY: &str = "chart-empty-state-icon";
/// The translated reason line.
pub const EMPTY_TEXT_KEY: &str = "chart-empty-state-text";

/// React `h-12 w-12` — the glyph is 48pt square.
const ICON_SIZE: f32 = 48.0;
/// React `gap-3` — 12pt between glyph and line.
const GAP: f32 = 12.0;
/// React `opacity-30`, carried as the icon tint's alpha.
const ICON_ALPHA: f32 = 0.3;

/// Mirror of `ChartEmptyStateProps['reason']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // the ChartView track (T104) is the production caller
pub enum ChartEmptyReason {
    NoNumericField,
    NoData,
    NoConfig,
}

#[allow(dead_code)] // the ChartView track (T104) is the production caller
impl ChartEmptyReason {
    /// The TS union member, verbatim.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoNumericField => "noNumericField",
            Self::NoData => "noData",
            Self::NoConfig => "noConfig",
        }
    }

    /// The key React's `messages` record maps this reason to.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::NoNumericField => "chart.empty.noNumericField",
            Self::NoData => "chart.empty.noData",
            Self::NoConfig => "chart.empty.noConfig",
        }
    }

    /// Parse a TS reason; unknown values are not React states, so the caller
    /// decides — the mirror deliberately has no silent fallback.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "noNumericField" => Some(Self::NoNumericField),
            "noData" => Some(Self::NoData),
            "noConfig" => Some(Self::NoConfig),
            _ => None,
        }
    }
}

/// The centered "nothing to chart" pane.
pub struct ChartEmptyState {
    reason: ChartEmptyReason,
}

impl ChartEmptyState {
    pub fn new(reason: ChartEmptyReason) -> Self {
        Self { reason }
    }

    /// Builds the pane. React's `flex-1` grows into the space the parent
    /// leaves; the glyph and the line are centred inside it.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let muted = k.c.text_muted; // React `text-fg-muted`
        col()
            .key(EMPTY_KEY)
            .w_full()
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .items_center()
            .justify_center()
            .gap(GAP)
            .child(
                icon(&ICON_CHART_COLUMN, ICON_SIZE)
                    .key(EMPTY_ICON_KEY)
                    .color(muted.alpha(ICON_ALPHA)),
            )
            .child(
                text(t(self.reason.message_key()))
                    .key(EMPTY_TEXT_KEY)
                    .font(k.ty.body) // React `text-sm` is 14px
                    .color(muted)
                    .wrap(),
            )
    }
}
