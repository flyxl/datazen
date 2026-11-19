//! The wapps management page's view models — the slice of
//! `windows/wapps/WappManagementPage.tsx`'s module scope that other wapps views
//! and the state share: the installed-wapp row, the API-version handshake, the
//! workspace/theme contribution predicates, and the filter/search/counts/
//! grouping derivations (the React file's `WappSummary` helpers, its
//! `PluginFilter` type and its three `useMemo`s, flattened to functions of the
//! state they derive from).
//!
//! React keeps the installed list in `stores/wappStore.ts` behind the host's
//! `list_wapps` command; the Rune store surface is [`AppState::wapps`]
//! (filled by the seam transition `wapps_loaded`, honest about the missing
//! adapter method — see `wapps_state.rs`). This module only projects and
//! derive; it owns no state.

use crate::state_def::AppState;

/// `types/wapp.ts`'s `WAPP_API_VERSION`: the manifest handshake the card's
/// mismatch badge reads. Must match the host's `WAPP_API_VERSION`.
pub(crate) const WAPP_API_VERSION: u32 = 2;

/// React's `PluginFilter`: which contribution kind the page lists.
/// The content body defaults to Workspace (`useState(() =>
/// savedSnapshot?.filter ?? 'workspace')`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WappFilter {
    All,
    #[default]
    Workspace,
    Theme,
}

impl WappFilter {
    /// The three chips, in React's render order.
    pub(crate) const ALL: [Self; 3] = [Self::All, Self::Workspace, Self::Theme];

    /// The chip's key (`extension-filter-<f>`).
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::All => "extension-filter-all",
            Self::Workspace => "extension-filter-workspace",
            Self::Theme => "extension-filter-theme",
        }
    }

    /// The chip's i18n key (React's three-branch `t()` call).
    pub(crate) const fn label_key(self) -> &'static str {
        match self {
            Self::All => "extensions.page.filterAll",
            Self::Workspace => "extensions.page.filterWorkspace",
            Self::Theme => "extensions.page.filterTheme",
        }
    }
}

/// One page contribution of an installed wapp — the fields the page's Open
/// resolve and the workspace-tab payload read (`types/wapp.ts`'s
/// `WappPageSummary`: id, title and the optional page icon the workspace
/// pages derivation carries onto the entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WappPageSummaryVm {
    pub(crate) id: String,
    pub(crate) title: String,
    /// The page contribution's optional icon path (`toEntry`'s `page.icon`).
    pub(crate) icon: Option<String>,
}

/// One installed workspace app — the honest projection of the host's
/// `WappSummary` payload (`types/wapp.ts`) over the fields this page renders:
/// the card's identity, badges and actions, and the page's filter counts.
/// `themes` carries the theme ids (presence is all the page reads);
/// `ai_tools` never reaches this page and is not mirrored.
#[derive(Clone, Debug)]
pub(crate) struct WappSummaryVm {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    /// The manifest's `apiVersion`; [`WAPP_API_VERSION`] is the handshake.
    pub(crate) api_version: u32,
    pub(crate) author: Option<String>,
    pub(crate) description: Option<String>,
    /// The optional package-level icon path. The card's image branch is the
    /// `read_wapp_file` host seam (no adapter method), so the letter avatar
    /// renders whatever it declares; the field keeps the payload honest.
    pub(crate) icon: Option<String>,
    pub(crate) enabled: bool,
    /// The requested permission strings, one badge each.
    pub(crate) permissions: Vec<String>,
    pub(crate) pages: Vec<WappPageSummaryVm>,
    /// Theme contribution ids; non-empty means the theme badge/hint render.
    pub(crate) themes: Vec<String>,
}

/// React's `hasPages` (`p.pages.length > 0`).
pub(crate) fn has_pages(wapp: &WappSummaryVm) -> bool {
    !wapp.pages.is_empty()
}

/// React's `hasThemes` (`p.themes.length > 0`).
pub(crate) fn has_themes(wapp: &WappSummaryVm) -> bool {
    !wapp.themes.is_empty()
}

/// React's `matchesFilter`.
pub(crate) fn matches_filter(wapp: &WappSummaryVm, filter: WappFilter) -> bool {
    match filter {
        WappFilter::Workspace => has_pages(wapp),
        WappFilter::Theme => has_themes(wapp),
        WappFilter::All => true,
    }
}

/// Whether `wapp` matches the search text — React's
/// `p.name.toLowerCase().includes(q) || p.id... || (p.description ?? '')...`,
/// with the trimmed lowercase needle already prepared by the caller.
fn matches_search(wapp: &WappSummaryVm, needle: &str) -> bool {
    needle.is_empty()
        || wapp.name.to_lowercase().contains(needle)
        || wapp.id.to_lowercase().contains(needle)
        || wapp
            .description
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(needle)
}

/// The page's `filtered` useMemo: the filter chip and the search box together
/// (`WappManagementPage.tsx:145-155`).
pub(crate) fn filtered_wapps(state: &AppState) -> Vec<&WappSummaryVm> {
    let needle = state.wapp_search.text.trim().to_lowercase();
    state
        .wapps
        .iter()
        .filter(|wapp| matches_filter(wapp, state.wapp_filter) && matches_search(wapp, &needle))
        .collect()
}

/// The page's `counts` useMemo — over the whole installed list, not the
/// filtered one (`WappManagementPage.tsx:157-164`): the chips say how many of
/// each kind exist, whatever the search shows.
pub(crate) fn wapp_counts(wapps: &[WappSummaryVm]) -> (usize, usize, usize) {
    (
        wapps.len(),
        wapps.iter().filter(|w| has_pages(w)).count(),
        wapps.iter().filter(|w| has_themes(w)).count(),
    )
}

/// The page's `allGroups` useMemo: the "all" view's two sections — the
/// workspace pages first, then the theme-only wapps (a wapp contributing both
/// kinds belongs to the Workspace group and shows exactly once). `None`
/// unless the filter is `all`, exactly as the memo's guard does.
pub(crate) fn all_wapp_groups<'a>(
    filtered: &[&'a WappSummaryVm],
    filter: WappFilter,
) -> Option<[(&'static str, Vec<&'a WappSummaryVm>); 2]> {
    if filter != WappFilter::All {
        return None;
    }
    Some([
        (
            "workspace",
            filtered.iter().copied().filter(|w| has_pages(w)).collect(),
        ),
        (
            "theme",
            filtered
                .iter()
                .copied()
                .filter(|w| !has_pages(w) && has_themes(w))
                .collect(),
        ),
    ])
}
