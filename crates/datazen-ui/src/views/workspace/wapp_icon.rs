//! Port of `packages/frontend/src/windows/workspace/WappIcon.tsx`.
//!
//! A workspace-app contributed icon: the wapp's manifest icon through the
//! wapp asset protocol, or the lucide puzzle glyph when the manifest
//! contributes no icon or the asset fails to load.
//!
//! ## What maps where
//!
//! - `useState(false)`/`useEffect(() => setBroken(false), [icon])` become
//!   [`WappIconState`], kept in the owning state `S`: the effect's dep is the
//!   `icon` prop's *value*, so [`WappIconState::sync_icon`] resets `broken`
//!   only when that value changes (to, from or between `None`), and the
//!   `<img onError>` handler is [`WappIconState::on_error`]. The host calls
//!   `sync_icon` each frame with the current prop before building the view.
//! - `<img src={buildWappIconSrc(targetId, icon)} onError={...}>` is two
//!   things. The **src** is [`build_icon_src`] — byte-identical to React's
//!   `DESKTOP_WAPP_ASSET_RESOLVER.buildIconSrc` (`datazen://{id}/{path}`,
//!   one leading `./` stripped), because `lib/wappAssetOrigin.ts` (the
//!   dual-form resolver registry with its web binding) is another segment's
//!   task and is not ported here; when it lands, this seam re-points to it
//!   and the React tests pin only the desktop default. The **picture** is a
//!   host concern exactly like kit markdown's `images` wire: the caller
//!   injects [`WappIconImages`] (`Fn(src) -> Option<Image>` from the host's
//!   `ImageCache`), and the view draws it with
//!   `.pixels_fit(.., SurfaceFit::Fit)` — CSS `object-contain`'s letterbox —
//!   in an explicit `size_pt` square. A decoding image draws blank, as an
//!   `<img>` does before its bytes arrive; a resolver answer of `None`, or an
//!   `Image` whose `error()` is set, is this port's `onError`: the puzzle
//!   shows that same frame (immediate mode needs no stale broken frame), and
//!   the host records the failure with `WappIconState::on_error` so the
//!   fallback stays sticky even if the resolver later finds the picture —
//!   React's `broken` survives a re-render with the same `icon` value.
//! - `className` sizes are explicit points: every React caller passes a
//!   Tailwind size (`h-3.5 w-3.5` 14, `h-4 w-4` 16, `h-5 w-5` 20), so
//!   [`WappIcon::size_pt`] takes the number (default 16, the modal caller's
//!   size; Rune has no class inheritance). `shrink-0` is `.shrink()`;
//!   `inline-block` has no Rune counterpart (an `El` is the leaf).
//! - `Puzzle` is `icons.rs`'s registered puzzle (`ICON_WAPPS`, the same
//!   lucide 0.487.0 body the rail's Wapps section uses — the registry
//!   forbids a second copy). `text-fg-muted` maps to the kit's
//!   `text_muted` semantic token; React's exact `#8498af` is a theme hex the
//!   Rune side does not reproduce (known delta, as with every port's muted
//!   text).
//! - `aria-hidden` (the puzzle) and `alt=""` (the picture, decorative) both
//!   map to `.a11y_hidden()`. `draggable={false}` has no Rune counterpart —
//!   nothing is draggable unless `drag_out` is wired.
//!
//! ## Keys
//!
//! React has no `data-testid` here; the parts derive from the component key
//! (default `"wapp-icon"`): the puzzle fallback is `<key>` and the picture
//! element is `<key>-picture`. The `src` the picture would load is
//! observable through [`WappIcon::kind`] without a cache.
//!
//! Known deltas (documented, not silent): sizes are explicit points, not
//! Tailwind classes; the sticky fallback after a failed load is enforced by
//! the host's `WappIconState` projection rather than an in-component render
//! closure; the web resolver form (`/wapps/{id}/…`) waits for the
//! `lib/wappAssetOrigin.ts` port.

use std::rc::Rc;

use rune::{el, icon, El, Image, SurfaceFit};

use crate::icons::ICON_WAPPS;
use crate::ui::kit::kit;

/// The host's picture wire: a built icon src (`datazen://…`) to a decoded or
/// decoding [`Image`] from the host's cache, or `None` when the host has no
/// picture for it. Kit markdown's `images` callback is the pattern.
pub(crate) type WappIconImages = Rc<dyn Fn(&str) -> Option<Image>>;

/// React's `useState(false)` plus the `useEffect` that resets it, kept in
/// the owning state `S` (one per icon site the host shows).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
pub(crate) struct WappIconState {
    broken: bool,
    /// The effect dep: the last `icon` prop value this state synced to.
    last_icon: Option<String>,
}

#[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
impl WappIconState {
    /// The `useEffect(() => setBroken(false), [icon])`: a changed `icon`
    /// value — including a change to or from `None` — recovers the picture.
    /// The same value again is a no-op, which is what keeps a failed load
    /// sticky in React.
    pub(crate) fn sync_icon(&mut self, icon: Option<&str>) {
        if self.last_icon.as_deref() != icon {
            self.last_icon = icon.map(str::to_owned);
            self.broken = false;
        }
    }

    /// The `<img onError={() => setBroken(true)}>` handler.
    pub(crate) fn on_error(&mut self) {
        self.broken = true;
    }

    /// Whether the site currently shows the fallback for a failed load.
    pub(crate) fn is_broken(&self) -> bool {
        self.broken
    }
}

/// React `DESKTOP_WAPP_ASSET_RESOLVER.buildIconSrc`: the desktop wapp asset
/// protocol's icon URL, the path's one leading `./` stripped. The web form
/// and the resolver registry are `lib/wappAssetOrigin.ts`'s own port (not
/// this segment); this seam re-points there when it lands.
#[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
pub(crate) fn build_icon_src(target_id: &str, icon: &str) -> String {
    format!("datazen://{}/{}", target_id, strip_leading_dot_slash(icon))
}

/// React `stripLeadingDotSlash`: `path.replace(/^\.\//, '')` — one leading
/// `./`, not repeated ones.
fn strip_leading_dot_slash(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

/// The pure render decision before any picture is looked up — the part the
/// React tests pin without a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
pub(crate) enum WappIconKind {
    /// The puzzle glyph: no icon contributed, or the load failed.
    Fallback,
    /// The picture element with the src it asks the host for.
    Picture { src: String },
}

/// The `WappIcon` React props plus the Rune seams: the explicit size, the
/// `broken` projection of [`WappIconState`] and the host's picture wire.
pub(crate) struct WappIcon {
    key: String,
    /// React `wappId || ''` (empty string is falsy too, so both `None` and
    /// `""` collapse to the empty target id).
    target_id: String,
    icon: Option<String>,
    size_pt: f32,
    broken: bool,
    images: Option<WappIconImages>,
}

impl WappIcon {
    /// React `WappIconProps` (`wappId?`, `icon?`).
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn new(wapp_id: Option<&str>, icon: Option<&str>) -> Self {
        Self {
            key: "wapp-icon".to_owned(),
            target_id: wapp_id.filter(|id| !id.is_empty()).unwrap_or_default().to_owned(),
            icon: icon.filter(|icon| !icon.is_empty()).map(str::to_owned),
            size_pt: 16.0,
            broken: false,
            images: None,
        }
    }

    /// The element key prefix (default `"wapp-icon"`); sites that show
    /// several icons pass their own so the tree stays addressable.
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// The `className` size as points: `h-3.5 w-3.5` 14, `h-4 w-4` 16
    /// (default), `h-5 w-5` 20.
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn size_pt(mut self, size_pt: f32) -> Self {
        self.size_pt = size_pt;
        self
    }

    /// The `WappIconState::is_broken` projection of the owning state `S`.
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn broken(mut self, broken: bool) -> Self {
        self.broken = broken;
        self
    }

    /// The host's picture wire ([`WappIconImages`]).
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn images(mut self, images: WappIconImages) -> Self {
        self.images = Some(images);
        self
    }

    /// The branch React picks between the puzzle and the `<img>`, pure of
    /// any cache: `!icon || broken` is the fallback, else the picture with
    /// its src. (An empty-string icon is falsy in React, so it falls back.)
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn kind(&self) -> WappIconKind {
        match self.icon.as_deref() {
            None => WappIconKind::Fallback,
            Some(_) if self.broken => WappIconKind::Fallback,
            Some(icon) => WappIconKind::Picture { src: build_icon_src(&self.target_id, icon) },
        }
    }

    /// Build the icon at the given theme.
    #[allow(dead_code)] // reusable port; wired by the workspace wapp tracks
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        match self.kind() {
            WappIconKind::Fallback => self.puzzle(k),
            WappIconKind::Picture { src } => {
                // The picture wire answers for the src; `None` (the host has
                // no picture) and an errored `Image` (the load failed) are
                // React's onError — the puzzle this frame, and the host
                // records it with `WappIconState::on_error`.
                let picture = self
                    .images
                    .as_ref()
                    .and_then(|images| images(&src))
                    .filter(|image| image.error().is_none());
                match picture {
                    None => self.puzzle(k),
                    Some(image) => el()
                        .key(format!("{}-picture", self.key))
                        .size(self.size_pt, self.size_pt)
                        .shrink()
                        .a11y_hidden() // alt="": decorative
                        .pixels_fit(image.pixels(), SurfaceFit::Fit), // object-contain
                }
            }
        }
    }

    /// The lucide puzzle fallback: registry glyph, muted foreground, hidden
    /// from the a11y tree (`aria-hidden`), never shrunk away (`shrink-0`).
    fn puzzle(&self, k: &'static rune_kit::Kit) -> El {
        icon(&ICON_WAPPS, self.size_pt)
            .key(self.key.clone())
            .color(k.c.text_muted)
            .shrink()
            .a11y_hidden()
    }
}
