//! Port of `packages/frontend/src/windows/settings/AppearanceSection.tsx`
//! (T222) — the settings window's appearance section.
//!
//! Two selects and three honesty lines: the color scheme
//! (light/dark/system, the mode half of `settings.theme`), the theme
//! pack (the built-in default plus every theme contributed by an
//! **enabled** wapp), the orphan hint when the persisted wapp theme is
//! no longer offered, and the apply error. The trailing block reserves
//! the future appearance options (density, font size, …).
//!
//! ## What maps where
//!
//! - `encodeWappThemePackId` / `parseWappThemePackId` are
//!   `lib/themePackApply.ts`'s (T633, segment 2) — the two pure
//!   functions are carried here until that task absorbs them
//!   (`wapp:{wappId}:{themeId}`, the strict parse).
//! - `useWappStore`'s mount effect (`fetch()` — the theme list must not
//!   stay an outdated snapshot when a wapp was installed in another
//!   window) is the host's section-entry step, a seam.
//! - `collectThemeOptions` keeps its React rule: only themes of
//!   **enabled** wapps are switchable here.
//! - `handleModeChange` keeps the pack (`{...theme, mode}`); the
//!   `onThemeChange` prop (a draft override) is the same callback the
//!   host routes when it owns the settings draft — the
//!   `draftSettings ?? storedSettings` collapse.
//! - `handleThemeChange`'s no-op guard (`packId === theme.packId`
//!   re-applies nothing) reads the preference the render closed over —
//!   exactly React's per-render closure; the choose closure captures
//!   this build's snapshot.
//! - The orphan check: a pack id that parses as a wapp theme but is not
//!   in the offered list — the wapp was uninstalled or disabled.
//! - `SectionTitle`/`SettingRow` (settingsUi.tsx, T399) and
//!   `SettingHint` (T381) stand in as the settings subtree's shared
//!   precedent does: the 13/600 muted title, the 160pt label column,
//!   the "?" tooltip trigger.
//!
//! Known deltas (documented, not silent): `text-amber-500` maps the
//! kit's warning ink. Hosts without a theme-pack adapter report the active
//! built-in palette as read-only, without advertising third-party selection.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `appearance-section`,
//! `appearance-color-scheme-select`, `appearance-theme-select`,
//! `appearance-orphan-hint`, `appearance-error`. Unlabeled parts
//! derive: the scheme row `<key>-scheme-row`, the pack row
//! `<key>-pack-row`, the reserved header `<key>-more`.

use std::rc::Rc;

use rune::{col, row, text, tok, El, TextStyle};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::views::settings::setting_hint::SettingHint;
use crate::views::settings::settings_ui::{section_title, setting_row};

// CSS h-9 and text-sm/leading-5 in the original Settings fields.
const FIELD_HEIGHT: f32 = 36.0;
const FIELD_RADIUS: f32 = 9.0;
const FIELD_TEXT: TextStyle = TextStyle::new(14.0, 400.0).lh(20.0);

/// Appearance uses the original Select's real border box, not the kit's
/// half-point outside input shadow. Keep the kit's anchored listbox,
/// accessibility, selection handlers and keyboard focus; change only the
/// Appearance trigger skin.
fn appearance_select(select: Select, kit: &rune_kit::Kit, layer: &mut Vec<El>) -> El {
    let mut field_kit = (*kit).clone();
    field_kit.elevation = field_kit.elevation.with_input(Vec::<rune::Shadow>::new());
    field_kit.ty.ui_compact = FIELD_TEXT;
    field_kit.c.icon_muted = kit.c.text_faint;
    select
        .build(&field_kit, layer)
        .h(FIELD_HEIGHT)
        .radius(FIELD_RADIUS)
        .border(1.0, kit.c.border_base)
        // The kit already puts 8pt before the value. CSS px-2.5 plus
        // the 1px inside border yields 11px. The kit's larger chevron
        // frame needs a 2pt larger right inset to align its center.
        .pl(3.0)
        .pr(8.0)
}

/// The sentinel option value representing the built-in default theme
/// (packId = null).
pub const BUILTIN_PACK_VALUE: &str = "__builtin__";

/// `WAPP_THEME_PACK_PREFIX` (`wapp:`).
pub const WAPP_THEME_PACK_PREFIX: &str = "wapp:";

/// `encodeWappThemePackId` (carried until T633 absorbs it).
pub fn encode_wapp_theme_pack_id(wapp_id: &str, theme_id: &str) -> String {
    format!("{WAPP_THEME_PACK_PREFIX}{wapp_id}:{theme_id}")
}

/// `parseWappThemePackId` (carried): strict — the prefix, a separator
/// beyond position 0, and a non-empty theme id, else `None`.
pub fn parse_wapp_theme_pack_id(pack_id: Option<&str>) -> Option<(String, String)> {
    let rest = pack_id?.strip_prefix(WAPP_THEME_PACK_PREFIX)?;
    let sep = rest.find(':')?;
    if sep == 0 || sep == rest.len() - 1 {
        return None;
    }
    Some((rest[..sep].to_owned(), rest[sep + 1..].to_owned()))
}

/// React `ThemeMode` (`'light' | 'dark' | 'system'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl ThemeMode {
    /// The wire spelling (the settings contract's value).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    /// The wire spelling back into a mode. An unknown value is refused by
    /// `Setting::validate`, so this is only the read side's honest fallback.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    /// The i18n key of the select's label.
    fn label_key(self) -> &'static str {
        match self {
            Self::Light => "theme.light",
            Self::Dark => "theme.dark",
            Self::System => "theme.system",
        }
    }

    /// React's `MODE_OPTIONS`, in declaration order.
    pub const MODES: [ThemeMode; 3] = [Self::Light, Self::Dark, Self::System];
}

/// React `ThemePreference` (`{ mode, packId }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePreference {
    pub mode: ThemeMode,
    /// `None` is the built-in default.
    pub pack_id: Option<String>,
}

impl Default for ThemePreference {
    /// `DEFAULT_THEME_PREFERENCE`: dark, builtin.
    fn default() -> Self {
        Self { mode: ThemeMode::Dark, pack_id: None }
    }
}

/// One theme a wapp contributes (the `WappSummary.themes` entry's
/// visible half).
#[derive(Clone, Debug)]
pub struct AppearanceWappTheme {
    pub id: String,
    pub name: String,
}

/// One wapp row (the fields the theme options read).
#[derive(Clone, Debug)]
pub struct AppearanceWappVm {
    pub id: String,
    pub enabled: bool,
    pub themes: Vec<AppearanceWappTheme>,
}

/// One flattened theme option (React `ThemeOption`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeOption {
    pub pack_id: String,
    pub label: String,
}

/// `collectThemeOptions`: flatten **enabled** wapps into a single
/// theme-option list — a disabled wapp's themes are not switchable.
pub fn collect_theme_options(wapps: &[AppearanceWappVm]) -> Vec<ThemeOption> {
    wapps
        .iter()
        .filter(|wapp| wapp.enabled)
        .flat_map(|wapp| {
            wapp.themes.iter().map(|theme| ThemeOption {
                pack_id: encode_wapp_theme_pack_id(&wapp.id, &theme.id),
                label: theme.name.clone(),
            })
        })
        .collect()
}

/// What a pack choice resolved to (`handleThemeChange`'s halves, the
/// no-op guard applied).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackChoice {
    /// The builtin sentinel — `packId = null`.
    Builtin,
    /// A wapp theme's encoded pack id.
    Pack(String),
}

/// Resolve a pick against the active preference (the guard, pure):
/// `None` when the pick equals the active pack (the sentinel while
/// builtin included) — nothing to apply.
pub fn resolve_pack_choice(value: &str, active: &ThemePreference) -> Option<PackChoice> {
    let pack_id: Option<&str> = if value == BUILTIN_PACK_VALUE { None } else { Some(value) };
    if pack_id == active.pack_id.as_deref() {
        return None;
    }
    match pack_id {
        None => Some(PackChoice::Builtin),
        Some(id) => Some(PackChoice::Pack(id.to_owned())),
    }
}

/// The appearance section (React `AppearanceSection`).
pub struct AppearanceSection<'a, S: 'static> {
    key: &'a str,
    theme: &'a ThemePreference,
    wapps: &'a [AppearanceWappVm],
    /// The last apply error (the red line).
    error: Option<&'a str>,
    modes: &'a [ThemeMode],
    show_theme_packs: bool,
    on_mode_change: Option<Rc<dyn Fn(&mut S, ThemeMode)>>,
    on_pack_change: Option<Rc<dyn Fn(&mut S, PackChoice)>>,
    /// Which select's listbox is open (`None` closed, the single-open
    /// rule the two Selects share).
    open_select: Option<Rc<dyn Fn(&S) -> Option<usize>>>,
    set_open: Option<Rc<dyn Fn(&mut S, Option<usize>)>>,
}

impl<'a, S: 'static> AppearanceSection<'a, S> {
    pub fn new(theme: &'a ThemePreference, wapps: &'a [AppearanceWappVm]) -> Self {
        Self {
            key: "appearance-section",
            theme,
            wapps,
            error: None,
            modes: &ThemeMode::MODES,
            show_theme_packs: true,
            on_mode_change: None,
            on_pack_change: None,
            open_select: None,
            set_open: None,
        }
    }

    /// Restrict choices to the preferences the active host can persist and apply.
    pub fn supported_modes(mut self, modes: &'a [ThemeMode]) -> Self {
        self.modes = modes;
        self
    }

    pub fn show_theme_packs(mut self, show: bool) -> Self {
        self.show_theme_packs = show;
        self
    }

    /// The apply error line (`updateSettings`' rejection message).
    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    /// A mode choice (`handleModeChange`: the pack is kept — the host
    /// writes `{...theme, mode}`).
    pub fn on_mode_change(mut self, f: impl Fn(&mut S, ThemeMode) + 'static) -> Self {
        self.on_mode_change = Some(Rc::new(f));
        self
    }

    /// A pack choice (`handleThemeChange`'s resolved half — the guard
    /// already ran; `Unchanged` never reaches the host).
    pub fn on_pack_change(mut self, f: impl Fn(&mut S, PackChoice) + 'static) -> Self {
        self.on_pack_change = Some(Rc::new(f));
        self
    }

    /// The listbox plumbing: which of the two selects is open (None
    /// closed; 0 the scheme, 1 the pack) and the setter.
    pub fn listbox(
        mut self,
        open: impl Fn(&S) -> Option<usize> + 'static,
        set_open: impl Fn(&mut S, Option<usize>) + 'static,
    ) -> Self {
        self.open_select = Some(Rc::new(open));
        self.set_open = Some(Rc::new(set_open));
        self
    }

    /// Build the section (tooltips land in `layer`).
    pub fn build(self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let key = self.key;

        let theme_options = collect_theme_options(self.wapps);
        let active_pack_id = self.theme.pack_id.as_deref();
        let active_plugin_theme_missing = parse_wapp_theme_pack_id(active_pack_id).is_some()
            && !theme_options.iter().any(|option| Some(option.pack_id.as_str()) == active_pack_id);

        let mut section = col().key(key.to_owned()).w_full().gap(16.0).child(section_title(
            &k,
            layer,
            format!("{key}-title"),
            t("settings.appearance"),
            Some(t("settings.appearance.subtitle")),
        ));

        // -- The color scheme row --
        let scheme_labels: Vec<String> =
            self.modes.iter().map(|mode| t(mode.label_key()).to_owned()).collect();
        let scheme_values: Vec<String> =
            self.modes.iter().map(|mode| mode.as_str().to_owned()).collect();
        let mode_label = t(self.theme.mode.label_key()).to_owned();
        let mut scheme_select = Select::new("appearance-color-scheme-select", &scheme_labels)
            .values(&scheme_values)
            .current(Some(&mode_label))
            .label(t("settings.colorScheme"))
            .open(open_flag::<S>(&self.open_select, s) == Some(0));
        if let (Some(set_open), open_slot) = (&self.set_open, self.open_select.clone()) {
            let toggle_open = Rc::clone(set_open);
            let close_open = Rc::clone(set_open);
            scheme_select = scheme_select
                .on_toggle(move |state: &mut S| {
                    let next = if open_flag(&open_slot, state) == Some(0) { None } else { Some(0) };
                    toggle_open(state, next);
                })
                .on_close(move |state: &mut S| close_open(state, None));
        }
        if let (Some(on_mode), set_open) = (&self.on_mode_change, self.set_open.clone()) {
            let on_mode = Rc::clone(on_mode);
            let modes = self.modes.to_vec();
            scheme_select =
                scheme_select.on_choose(move |state: &mut S, value: &str, _tx: &mut rune::Tx| {
                    if let Some(mode) = modes.iter().find(|mode| mode.as_str() == value)
                    {
                        on_mode(state, *mode);
                    }
                    // The React Select closes its listbox on a choose.
                    if let Some(set_open) = &set_open {
                        set_open(state, None);
                    }
                });
        }
        let scheme_control = appearance_select(scheme_select, k, layer);
        section = section.child(setting_row(
            &k,
            layer,
            format!("{key}-scheme-row"),
            t("settings.colorScheme"),
            None,
            scheme_control,
        ));

        if self.show_theme_packs {
            // -- The theme pack row --
            let mut pack_labels = vec![t("settings.theme.packDefault").to_owned()];
            pack_labels.extend(theme_options.iter().map(|option| option.label.clone()));
            let mut pack_values = vec![BUILTIN_PACK_VALUE.to_owned()];
            pack_values.extend(theme_options.iter().map(|option| option.pack_id.clone()));
            // The current the kit matches by LABEL: the builtin default's
            // label, or the active option's label.
            let current_pack_label = active_pack_id
                .and_then(|id| {
                    theme_options.iter().find(|option| option.pack_id == id).map(|option| option.label.clone())
                })
                .unwrap_or_else(|| t("settings.theme.packDefault").to_owned());
            let pack_hint = theme_options.is_empty().then(|| t("settings.appearance.emptyHint"));
            let mut pack_select = Select::new("appearance-theme-select", &pack_labels)
                .values(&pack_values)
                .current(Some(&current_pack_label))
                .label(t("settings.theme.pack"))
                .open(open_flag::<S>(&self.open_select, s) == Some(1));
            if let (Some(set_open), open_slot) = (&self.set_open, self.open_select.clone()) {
                let toggle_open = Rc::clone(set_open);
                let close_open = Rc::clone(set_open);
                pack_select = pack_select
                    .on_toggle(move |state: &mut S| {
                        let next = if open_flag(&open_slot, state) == Some(1) { None } else { Some(1) };
                        toggle_open(state, next);
                    })
                    .on_close(move |state: &mut S| close_open(state, None));
            }
            if let (Some(on_pack), set_open) = (&self.on_pack_change, self.set_open.clone()) {
                let on_pack = Rc::clone(on_pack);
                // The guard reads the preference this build closed over —
                // React's per-render closure, verbatim.
                let active = self.theme.clone();
                pack_select =
                    pack_select.on_choose(move |state: &mut S, value: &str, _tx: &mut rune::Tx| {
                        if let Some(choice) = resolve_pack_choice(value, &active) {
                            on_pack(state, choice);
                        }
                        if let Some(set_open) = &set_open {
                            set_open(state, None);
                        }
                    });
            }
            let pack_control = appearance_select(pack_select, k, layer);
            section = section.child(setting_row(
                &k,
                layer,
                format!("{key}-pack-row"),
                t("settings.theme.pack"),
                pack_hint,
                pack_control,
            ));

            // -- The orphan hint --
            if active_plugin_theme_missing {
                section = section.child(
                    text(t("settings.appearance.missingHint").to_owned())
                        .font(tok::type_::LABEL)
                        .color(k.c.warning.fg) // React text-amber-500
                        .key("appearance-orphan-hint"),
                );
            }
        } else if active_pack_id.is_none() {
            // This host actually uses the built-in palette. State the current
            // pack without manufacturing a focusable select or promising a
            // plugin install/apply action the host does not implement.
            let value = row()
                .key("appearance-builtin-theme-status")
                .w_full()
                .h(FIELD_HEIGHT)
                .px(11.0)
                .center_y()
                .radius(FIELD_RADIUS)
                .fill(k.c.bg_base)
                .border(1.0, k.c.border_base)
                .child(text(t("settings.theme.packDefault")).font(FIELD_TEXT).color(k.c.text_base));
            section = section.child(setting_row(
                &k,
                layer,
                format!("{key}-pack-row"),
                t("settings.theme.pack"),
                None,
                value,
            ));
        }

        // -- The apply error --
        if let Some(error) = self.error {
            section = section.child(
                text(t("settings.appearance.applyError").replace("{error}", error))
                    .font(tok::type_::LABEL)
                    .color(k.c.danger.fg)
                    .selectable(true)
                    .key("appearance-error"),
            );
        }

        // The section's 16pt gap plus this 16pt wrapper implements mt-8.
        // Rune's painted inside border does not reserve layout space, so the
        // 17pt content inset includes the CSS 1px border plus pt-4.
        let more_label = t("settings.appearance.more");
        let more = row()
            .gap(4.0)
            .center_y()
            .child(text(more_label).font(TextStyle::new(14.0, 500.0).lh(20.0)).color(k.c.text_muted))
            .child(
                SettingHint::new(
                    format!("{key}-more"),
                    more_label,
                    t("settings.appearance.morePlaceholder"),
                )
                .build(&k, layer),
            );
        section.child(col().pt(16.0).child(
            col()
                .key(format!("{key}-more"))
                .w_full()
                .pt(17.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
                .child(more),
        ))
    }
}

/// Read the open flag through the optional slot (closed without one).
fn open_flag<S: 'static>(
    slot: &Option<Rc<dyn Fn(&S) -> Option<usize>>>,
    s: &S,
) -> Option<usize> {
    slot.as_ref().and_then(|open| open(s))
}
