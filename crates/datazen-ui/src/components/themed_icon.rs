//! T262: the themed icon, ported from
//! `packages/frontend/src/components/ThemedIcon.tsx`.
//!
//! One semantic id (`db.postgresql`, `ui.settings`, …) through the icon
//! resolver: a theme-pack URL (an `<img>`), a lucide name (the
//! LUCIDE_MAP lookup, then the `fallback` prop, then the `?` badge), or
//! the placeholder badge (the two-letter db tile). The resolver itself
//! (`lib/iconResolver.ts` — `createIconResolver`, the active-resolver
//! module state, the bootstrap seed) is **T535's file, segment 2**: this
//! port defines the consuming seam — [`ResolvedIcon`] and the
//! [`IconResolver`] callback — and the default resolver's exact
//! module-level behavior (empty maps: every `db.*` id degrades to its
//! two-character placeholder, everything else to the `?`), so T535
//! lands the real resolver behind the same seam.
//!
//! | React piece | here |
//! |---|---|
//! | `ResolvedIcon` (url / lucide / placeholder) | [`ResolvedIcon`] |
//! | `IconResolver` / `getActiveIconResolver` | the [`ThemedIcon::resolver`] callback; `None` is [`default_resolver`] |
//! | `LUCIDE_MAP` (the 36-name catalog) | [`lucide_map_icon`] — the registry's constants, 20 newly registered from the lucide sources |
//! | the `fallback` prop | [`ThemedIcon::fallback_icon`] |
//! | the `?` badge (React 118–128) | the `bg-slate-600` badge — [`BadgeTone::Neutral`] |
//! | the placeholder badge (React 131–142) | the label on the resolver's tone (the `getDbIcon` bg is [`BadgeTone`] at the `db_badge_meta` mirror) |
//! | `<img src>` (React 102–111) | the host's [`ThemedIconImages`] picture wire, `.pixels_fit` `object-contain` (the `WappIcon` precedent) |
//! | `datazen:theme-pack-changed` re-render | free in immediate mode — the host's next frame re-resolves; the event seam lands with the theme-pack wiring |
//!
//! ## Key convention
//!
//! | part | key |
//! |---|---|
//! | the mounted element | `<key>` |
//!
//! The three branches are observable through [`ThemedIcon::kind`] (the
//! resolved variant) without a runtime, and through the mounted element:
//! the lucide branch's element carries no text, the placeholder carries
//! the label, the url branch's picture carries the host wire's pixels.
//!
//! ## Deliberate boundaries
//!
//! * `className` sizes are explicit points (`h-3.5 w-3.5` 14, `h-4
//!   w-4` 16, `h-5 w-5` 20 — every React caller passes a Tailwind size):
//!   [`ThemedIcon::size_pt`], default 16. `shrink-0` is no call at all
//!   (Rune children do not shrink); `inline-block` has no counterpart.
//! * The url branch's `draggable={false}` and `alt=""` are
//!   `.a11y_hidden()` and nothing (nothing is draggable unless wired);
//!   a `None` picture (the wire has no bytes yet) draws blank exactly as
//!   an `<img>` does before its bytes arrive — there is no onError
//!   fallback here, matching React (the `<img>` failure shows nothing).
//! * The placeholder's Tailwind bg class is [`BadgeTone`] here: the
//!   `getDbIcon` bgs are exactly the tones `db_badge_meta` mirrors, and
//!   the tone-to-fill projection is `db_type_badge`'s own.

use std::rc::Rc;

use rune::{el, icon, El, Image, SurfaceFit, TextStyle};

use crate::components::db_type_badge::{badge_font_size, badge_tone_fill};
use crate::icons::{
    ICON_CONNECTIONS, ICON_DASHBOARDS, ICON_TABLE_PROPERTIES, ICON_WAPPS, ICON_WORKFLOWS,
    ICON_WORKSPACE,
};
use crate::ui::badge::BadgeTone;
use crate::ui::kit::kit;

/// React's `ResolvedIcon` union (`lib/iconResolver.ts`): what one
/// semantic id resolved to — a lucide component name, a URL, or the
/// placeholder badge (its `bgClass` is the [`BadgeTone`] the tile
/// paints, mapped once at the `db_badge_meta` mirror).
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedIcon {
    Lucide(String),
    Url(String),
    Placeholder { label: String, tone: BadgeTone },
}

/// React's `IconResolver` interface — `resolve(semanticId)`. The
/// resolver's construction is T535's (segment 2); this is the seam the
/// consumers hold.
pub type IconResolver = Rc<dyn Fn(&str) -> ResolvedIcon>;

/// React's module-level default resolver (`iconResolver.ts`'s
/// `activeIconResolver` seed): empty `packIcons`, `driverIcons` and
/// `lucideById`, and `placeholderForDb` = the db type's first two
/// characters on slate — so a `db.*` id degrades to its two-character
/// tile and every other id to the `?` placeholder.
pub fn default_resolver(semantic_id: &str) -> ResolvedIcon {
    if let Some(db_type) = semantic_id.strip_prefix("db.") {
        return ResolvedIcon::Placeholder {
            label: db_type.chars().take(2).collect(),
            tone: BadgeTone::Neutral,
        };
    }
    ui_placeholder()
}

/// React's `UI_PLACEHOLDER`: the `?` on slate.
fn ui_placeholder() -> ResolvedIcon {
    ResolvedIcon::Placeholder {
        label: "?".to_owned(),
        tone: BadgeTone::Neutral,
    }
}

/// React's `LUCIDE_MAP`: the lucide component names the host semantic
/// catalog's defaults reference, to the registry's constants. The
/// 36-name set is React's exactly; 16 were already registered, and T262
/// registered the other 20 from the lucide sources.
pub fn lucide_map_icon(name: &str) -> Option<&'static rune::Icon> {
    use crate::icons as i;
    let icon = match name {
        "activity" => &i::ICON_ACTIVITY,
        "braces" => &i::ICON_BRACES,
        "bot" => &i::ICON_BOT,
        "cable" => &i::ICON_CABLE,
        "code-2" => &i::ICON_CODE,
        "database" => &ICON_CONNECTIONS,
        "database-backup" => &i::ICON_DATABASE_BACKUP,
        "download" => &i::ICON_DOWNLOAD,
        "eye" => &i::ICON_EYE,
        "file-text" => &i::ICON_FILE_TEXT,
        "folder-open" => &i::ICON_FOLDER_OPEN,
        "git-fork" => &i::ICON_GIT_FORK,
        "gauge" => &ICON_DASHBOARDS,
        "hash" => &i::ICON_HASH,
        "globe" => &i::ICON_GLOBE,
        "key-round" => &i::ICON_KEY_ROUND,
        "layout-grid" => &ICON_WORKSPACE,
        "message-square-text" => &i::ICON_MESSAGE_SQUARE_TEXT,
        "monitor" => &i::ICON_MONITOR,
        "moon" => &i::ICON_MOON,
        "mouse-pointer-click" => &i::ICON_MOUSE_POINTER_CLICK,
        "palette" => &i::ICON_PALETTE,
        "play" => &i::ICON_PLAY,
        "plug" => &i::ICON_PLUG,
        "plus" => &i::ICON_PLUS,
        "puzzle" => &ICON_WAPPS,
        "refresh-ccw" => &i::ICON_REFRESH_CCW,
        "server" => &i::ICON_SERVER,
        "shapes" => &i::ICON_SHAPES,
        "settings" => &i::ICON_SETTINGS,
        "square" => &i::ICON_SQUARE,
        "sun" => &i::ICON_SUN,
        "table-2" => &i::ICON_TABLE_2,
        "table-properties" => &ICON_TABLE_PROPERTIES,
        "zap" => &i::ICON_ZAP,
        "workflow" => &ICON_WORKFLOWS,
        _ => return None,
    };
    Some(icon)
}

/// The host's picture wire: a resolved URL to a decoded or decoding
/// [`Image`] from the host's cache, or `None` when the host has no
/// picture for it yet (the `WappIcon` images pattern).
pub type ThemedIconImages = Rc<dyn Fn(&str) -> Option<Image>>;

/// One themed icon.
pub struct ThemedIcon<S: 'static> {
    key: String,
    id: String,
    size_pt: f32,
    resolver: Option<IconResolver>,
    fallback_icon: Option<&'static rune::Icon>,
    images: Option<ThemedIconImages>,
    on_pack_changed: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> ThemedIcon<S> {
    /// `ThemedIcon::<S>::new("themed-icon", id)` — the key is this
    /// port's own (React has no `data-testid`).
    pub fn new(key: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            id: id.into(),
            size_pt: 16.0,
            resolver: None,
            fallback_icon: None,
            images: None,
            on_pack_changed: None,
        }
    }

    /// The icon's side in points (React's callers all pass a Tailwind
    /// size: 14, 16, or 20).
    pub fn size_pt(mut self, points: f32) -> Self {
        self.size_pt = points;
        self
    }

    /// React's `resolver` prop — `None` uses the default resolver's
    /// module behavior.
    pub fn resolver(mut self, resolver: IconResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// React's `fallback` — the lucide component used when the map has
    /// no entry for the resolved name.
    pub fn fallback_icon(mut self, icon: &'static rune::Icon) -> Self {
        self.fallback_icon = Some(icon);
        self
    }

    /// The url branch's picture wire.
    pub fn images(mut self, images: ThemedIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// The `datazen:theme-pack-changed` notification seam: the host
    /// runs it when the pack changes, exactly the bump React's listener
    /// performs. In immediate mode the next frame re-resolves anyway;
    /// this seam is for the host-side bookkeeping (cache flushes).
    #[allow(dead_code)] // the theme-pack wiring task is the caller
    pub fn on_pack_changed(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_pack_changed = Some(Rc::new(f));
        self
    }

    /// What `id` resolves to — the resolver's answer, without a
    /// runtime (React's `resolved`, for the parity tests).
    pub fn kind(&self) -> ResolvedIcon {
        match &self.resolver {
            Some(resolver) => resolver(&self.id),
            None => default_resolver(&self.id),
        }
    }

    /// Paint the icon: the three branches of React 102–143.
    pub fn build(self, dark: bool) -> El {
        match self.kind() {
            // React 102–111: the url — an `<img>` the host loads; here
            // the host's picture wire, letterboxed
            // (`object-contain`), blank until the bytes arrive.
            ResolvedIcon::Url(href) => {
                let picture = self.images.as_ref().and_then(|images| images(&href));
                let mut el = el()
                    .key(self.key)
                    .a11y_hidden()
                    .size(self.size_pt, self.size_pt);
                if let Some(image) = picture {
                    el = el.pixels_fit(image.pixels(), SurfaceFit::Fit);
                }
                el
            }
            // React 113–129: the lucide name — the map, then the
            // fallback, then the `?` badge.
            ResolvedIcon::Lucide(name) => {
                match lucide_map_icon(&name).or(self.fallback_icon) {
                    Some(glyph) => icon(glyph, self.size_pt)
                        .key(self.key)
                        // React 116: `aria-hidden`.
                        .a11y_hidden(),
                    None => badge_el(self.key, "?", dark, self.size_pt, BadgeTone::Neutral),
                }
            }
            // React 131–142: the placeholder badge — the label on the
            // resolver's tone.
            ResolvedIcon::Placeholder { label, tone } => {
                badge_el(self.key, &label, dark, self.size_pt, tone)
            }
        }
    }
}

/// One placeholder badge: React 118–128 / 131–142's `inline-flex
/// items-center justify-center rounded text-[10px] font-semibold
/// text-white` span on the tone's fill (or slate for the `?`).
fn badge_el(key: String, label: &str, dark: bool, size: f32, tone: BadgeTone) -> El {
    let k = kit(dark);
    let label_key = format!("{key}-label");
    let font = TextStyle {
        size: badge_font_size(size),
        weight: 600.0,
        line_height: 0.0,
        ..k.ty.text_xsmall
    };
    rune::row()
        .key(key)
        .a11y_hidden()
        .size(size, size)
        .items_center()
        .justify_center()
        .radius(4.0)
        .fill(badge_tone_fill(tone, dark))
        .child(
            rune::text(label)
                .key(label_key)
                .font(font)
                .color(k.c.white),
        )
}
