//! UI store ported from React `packages/frontend/src/stores/uiStore.ts`
//! (tasklist T471).
//!
//! The React module is a zustand store holding the shell's layout state:
//! four resizable sizes, the connections view mode, the single open dialog
//! slot, the fullscreen flag, and the workspace sidebar mode — the only
//! cell that persists (window.localStorage, best-effort). In immediate mode
//! the store is the state: the shell host owns one [`UiStore`] field and
//! the hook's setter calls become method calls.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useUiStore()` (the hook) | the host's `UiStore` field (single instance) |
//! | `mainSidebarWidth`/`connectionSidebarWidth`/`editorHeight`/`resultHeight` seeds `220/280/320/360` | the four `pub` cells, same seeds |
//! | `openDialog(id)` / `closeDialog()` | [`UiStore::open_dialog`] / [`UiStore::close_dialog`] over `Option<DialogId>` |
//! | `toggleWorkspaceSidebarMode` / `setWorkspaceSidebarMode` | same methods, taking the [`SidebarModeStorage`] port that `window.localStorage` becomes |
//! | `syncFullscreen()` (async, platform-probed) | [`UiStore::sync_fullscreen`] with an injected `Option<bool>` probe (see below) |
//!
//! `DialogId` is `'new-connection' | null`: the enum carries the one
//! non-null id and the slot is `Option` — `openDialog`'s
//! `Exclude<DialogId, null>` argument is the enum itself.
//!
//! `syncFullscreen` is async against the platform port
//! (`getPlatformOrNull()?.windows.windowChrome.isFullscreen()`, wrapped in
//! `try`/`catch`). Rune is immediate and synchronous, so the probe is
//! injected: `Some(value)` is a platform answer (the flag is set and the
//! value returned, exactly React's `set + return`), `None` is "no platform
//! or the call threw" — React's two early-`return false` paths, which leave
//! the stored flag untouched. An unconfigured platform therefore degrades
//! to "not fullscreen", matching the legacy browser branch the React doc
//! cites.
//!
//! Persistence: `window.localStorage` becomes the [`SidebarModeStorage`]
//! port. `loadWorkspaceSidebarMode`'s SSR guard (`typeof window ===
//! 'undefined' || !window.localStorage`) is the port returning `None`; the
//! stored value counts only when it is exactly `'expanded'`; `setItem` is
//! best-effort (the React `try`/`catch`), which the port's infallible
//! `set` signature encodes.

/// React's `DialogId = 'new-connection' | null`. The enum carries the one
/// non-null dialog id; the slot itself is `Option<DialogId>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the new-connection dialog track (not landed yet)
pub enum DialogId {
    /// The "new connection" modal (`'new-connection'`).
    NewConnection,
}

/// `ConnectionsViewMode`: how the connections home lists its entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the connections home track (not landed yet)
pub enum ConnectionsViewMode {
    #[default]
    Grid,
    List,
}

/// `WorkspaceSidebarMode`: the workspace sidebar's two shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub enum WorkspaceSidebarMode {
    /// Icon-only rail (the default; also the degraded/preference-less mode).
    #[default]
    Icons,
    /// Icon + label rail.
    Expanded,
}

/// `datazen:workspace-sidebar-mode`.
pub const WORKSPACE_SIDEBAR_MODE_KEY: &str = "datazen:workspace-sidebar-mode";

/// The `window.localStorage` port the sidebar mode persists through.
///
/// Reads answer `None` when the key is absent or storage is unavailable
/// (React's SSR guard); writes are best-effort and infallible — React's
/// `try { localStorage.setItem(...) } catch {}` — so a failing write is the
/// port's internal affair and never fails the caller.
#[allow(dead_code)] // consumers: the workspace shell track (not landed yet)
pub trait SidebarModeStorage {
    /// `localStorage.getItem(key)`.
    fn get(&self, key: &str) -> Option<String>;
    /// `localStorage.setItem(key, value)` (best-effort).
    fn set(&mut self, key: &str, value: &str);
}

/// `loadWorkspaceSidebarMode`: exactly `'expanded'` counts; an absent,
/// unavailable, or differently-spelled stored value falls back to `'icons'`.
pub fn load_workspace_sidebar_mode(stored: Option<&str>) -> WorkspaceSidebarMode {
    if stored == Some("expanded") {
        WorkspaceSidebarMode::Expanded
    } else {
        WorkspaceSidebarMode::Icons
    }
}

/// The store (`useUiStore`'s state): the shell's layout state.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // consumers: the shell/window tracks (not landed yet)
pub struct UiStore {
    /// `mainSidebarWidth` seed: 220.
    pub main_sidebar_width: u32,
    /// `connectionSidebarWidth` seed: 280.
    pub connection_sidebar_width: u32,
    /// `editorHeight` seed: 320.
    pub editor_height: u32,
    /// `resultHeight` seed: 360.
    pub result_height: u32,
    /// `connectionsViewMode` seed: `'grid'`.
    pub connections_view_mode: ConnectionsViewMode,
    /// `activeDialog` seed: `null`.
    pub active_dialog: Option<DialogId>,
    /// `isFullscreen` seed: `false`.
    pub is_fullscreen: bool,
    /// `workspaceSidebarMode` seed: `loadWorkspaceSidebarMode()`.
    pub workspace_sidebar_mode: WorkspaceSidebarMode,
}

#[allow(dead_code)] // consumers: the shell/window tracks (not landed yet)
impl UiStore {
    /// The module init: the four seeds plus the persisted sidebar mode
    /// read once from storage.
    pub fn new(storage: &dyn SidebarModeStorage) -> Self {
        Self {
            main_sidebar_width: 220,
            connection_sidebar_width: 280,
            editor_height: 320,
            result_height: 360,
            connections_view_mode: ConnectionsViewMode::Grid,
            active_dialog: None,
            is_fullscreen: false,
            workspace_sidebar_mode: load_workspace_sidebar_mode(
                storage.get(WORKSPACE_SIDEBAR_MODE_KEY).as_deref(),
            ),
        }
    }

    /// `setMainSidebarWidth(w)`.
    pub fn set_main_sidebar_width(&mut self, w: u32) {
        self.main_sidebar_width = w;
    }

    /// `setConnectionSidebarWidth(w)`.
    pub fn set_connection_sidebar_width(&mut self, w: u32) {
        self.connection_sidebar_width = w;
    }

    /// `setEditorHeight(h)`.
    pub fn set_editor_height(&mut self, h: u32) {
        self.editor_height = h;
    }

    /// `setResultHeight(h)`.
    pub fn set_result_height(&mut self, h: u32) {
        self.result_height = h;
    }

    /// `openDialog(id)`.
    pub fn open_dialog(&mut self, id: DialogId) {
        self.active_dialog = Some(id);
    }

    /// `closeDialog()`.
    pub fn close_dialog(&mut self) {
        self.active_dialog = None;
    }

    /// `setConnectionsViewMode(mode)`.
    pub fn set_connections_view_mode(&mut self, mode: ConnectionsViewMode) {
        self.connections_view_mode = mode;
    }

    /// `setFullscreen(value)`.
    pub fn set_fullscreen(&mut self, value: bool) {
        self.is_fullscreen = value;
    }

    /// `syncFullscreen()`: the platform-probed read. `Some(value)` is a
    /// platform answer — the flag is set and the value returned. `None`
    /// is "no platform / the call threw" — `false` is returned and the
    /// stored flag is left untouched.
    pub fn sync_fullscreen(&mut self, probe: impl FnOnce() -> Option<bool>) -> bool {
        match probe() {
            Some(value) => {
                self.is_fullscreen = value;
                value
            }
            None => false,
        }
    }

    /// `toggleWorkspaceSidebarMode()`: flip, persist the new mode, apply it.
    pub fn toggle_workspace_sidebar_mode(&mut self, storage: &mut dyn SidebarModeStorage) {
        let next = match self.workspace_sidebar_mode {
            WorkspaceSidebarMode::Icons => WorkspaceSidebarMode::Expanded,
            WorkspaceSidebarMode::Expanded => WorkspaceSidebarMode::Icons,
        };
        storage.set(
            WORKSPACE_SIDEBAR_MODE_KEY,
            match next {
                WorkspaceSidebarMode::Icons => "icons",
                WorkspaceSidebarMode::Expanded => "expanded",
            },
        );
        self.workspace_sidebar_mode = next;
    }

    /// `setWorkspaceSidebarMode(mode)`: persist, then apply.
    pub fn set_workspace_sidebar_mode(
        &mut self,
        mode: WorkspaceSidebarMode,
        storage: &mut dyn SidebarModeStorage,
    ) {
        storage.set(
            WORKSPACE_SIDEBAR_MODE_KEY,
            match mode {
                WorkspaceSidebarMode::Icons => "icons",
                WorkspaceSidebarMode::Expanded => "expanded",
            },
        );
        self.workspace_sidebar_mode = mode;
    }
}
