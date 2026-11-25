//! Port of `packages/frontend/src/App.tsx` (T051) — the root component: the
//! window router, the lazy/Suspense chrome around it, and the global hosts
//! mounted once for the whole app.
//!
//! React's `App` is one render with four layers:
//!
//! 1. the window router — `WindowContent` switches on `getWindowKind()`
//!    (`App.tsx:66-80`) across six window components;
//! 2. a lazy/Suspense pair — every window is a bundler chunk
//!    (`App.tsx:17-52`) with `WindowChromeFallback` as the interim chrome
//!    (`App.tsx:99`);
//! 3. the `ErrorBoundary` wrapper over all of it (`App.tsx:98`);
//! 4. three global hosts (`WebContextMenuHost`, `McpApprovalHost`,
//!    `AiWriteProposalHost`, `App.tsx:102-104`) and the four
//!    `globalTextSelection` installers (`App.tsx:84-95`).
//!
//! ## Branch table (`App.tsx`, line by line)
//!
//! | React | Rune |
//! |---|---|
//! | `lazy(() => import('./windows/main/MainPage'))` + `mark('MainPage chunk loaded')` (`App.tsx:17-22`) | direct composition: the native binary links the window's module, so there is no chunk to fetch and no load to measure. The `mark` belongs to the startup timer (T621). |
//! | `<Suspense fallback={<WindowChromeFallback />}>` (`App.tsx:99`) | no interim state exists — [`window_content`] returns the window on the first frame. `WindowChromeFallback.tsx` itself is T303 and has no reachable render here. |
//! | `WindowContent`'s switch (`App.tsx:66-80`) | [`window_content`]: the degenerate switch. The native host process is single-window and always the main window (`crates/datazen-rune-host/src/main.rs` calls `app_with_adapter` once; the kind resolver `lib/windowKind.ts` — a URL `?window=` param, a WebView routing concept — is T644). The other five arms are honest seams: `OnboardingWindow` T072, `DataSyncWindow` T270, `DataTransferWindow` T149, `SchemaDiffWindow` T309, `BackupWindow` T26. Each plugs into this switch when its task lands. |
//! | `case 'main': default: return <MainPage />` | `main_page` (T020, `views/main/main_page.rs`). |
//! | `useEffect` in `WindowContent`: `mark('window component mounted')` and `hideSplash` for non-main windows (`App.tsx:58-64`) | seams: the startup timer is T621 (dev-only `console.log`, no native counterpart yet) and the splash layer is T603 — the Rune process owns no splash element to hide, exactly as `views/main/main_page.rs` already records for the main window's own `hideSplash`. Only non-main windows hide it, and only the main window exists, so nothing is reachable today either way. |
//! | module scope: `const windowKind = getWindowKind()` + `mark('windowKind resolved')` (`App.tsx:54-55`) | seam: kind resolution is T644's; with one window there is nothing to resolve. |
//! | `<ErrorBoundary>` (`App.tsx:98`) | seam (T064): rune builds `El` trees before the frame — `vendor/rune` exposes no render-crash boundary a view could be wrapped in, so the wrapper layer has no native counterpart to compose. The React boundary's own chrome (`WindowChromeSlot`, `common.error` panel) belongs to T064. |
//! | `<WebContextMenuHost />` (`App.tsx:102`) | seam (T098): the global portal that renders `stores/contextMenuStore`'s menu. Rune's context menus are owned by their call sites through `ui/menu.rs`'s `context_menu`; no call site is ported yet, so the host has nothing to render. |
//! | `<McpApprovalHost />` (`App.tsx:103`) | seam (T171): the cross-window `mcp:approval-requested` queue — Rune is single-window with no event bus (`lib/crossWindowBus.ts` is T437), so the queue's source does not exist yet. |
//! | `<AiWriteProposalHost />` (`App.tsx:104`) | seam (T012): the `ai:write-proposal-requested` queue, same single-window/no-bus boundary as the MCP host. |
//! | the four `install*` selection installers (`App.tsx:84-95`) | no native counterpart — see below. |
//!
//! ## The four text-selection installers have no native counterpart
//!
//! `lib/globalTextSelection.ts` (T521) installs document-level DOM listeners
//! against the WebView's selection model: Cmd/Ctrl+A select-all suppression,
//! right-button drag-selection suppression, the native WebView context-menu
//! suppressor, and the stray left-drag selection guard. Rune renders its own
//! text and hit-testing: there is no document, no `window.getSelection()`,
//! no browser context menu and no drag-selection painting. The equivalent
//! concerns are owned by the widgets themselves (context menus by
//! `ui/menu.rs`, cell/label selection by the grid and text elements), so
//! there is nothing to install at the root — an honest no-op, attributed to
//! T521, in the same form as `main_page.rs`'s "the Rune process owns no
//! splash layer to hide".
//!
//! ## Keys
//!
//! The root adds none of its own: React's `App` renders its children
//! unwrapped (`ErrorBoundary` passes children through, `Suspense` adds no
//! node), so the root's tree begins at the routed window's own keys
//! (`datazen-root` and below, from `main_page`).
//!
//! ## Wiring
//!
//! `app.rs`'s `view` — the single entry both the demo `run()` and the host's
//! `run_with_adapter` hand the window to — routes through [`app_root`] (the
//! seam `tests/app_root.rs` pins against the sources, the i18n call-site
//! guard pattern: the routed output IS the main window's, so the tree cannot
//! tell routing from a direct call).

use crate::state_def::AppState;
use rune::{Cx, El};

use super::main_page;

/// The root composition (React `App`, `App.tsx:83-107`): the routed window
/// component under the boundary and suspense layers React wraps it in. Those
/// wrapper layers are the documented seams above; the body the window router
/// picks is composed directly, unwrapped, exactly as React renders it when
/// no error and no load is in flight.
pub fn app_root(state: &AppState, cx: &mut Cx<AppState>) -> El {
    window_content(state, cx)
}

/// The window router (React `WindowContent`, `App.tsx:57-81`): the switch
/// over the window kind. The native host runs one process for the main
/// window, so the switch's one reachable arm is `main`; the mount-time
/// effect beside it (the startup mark and the non-main `hideSplash`) is
/// unreachable for the same reason — the seams are recorded in the module
/// docs above. The five other arms land here as their windows are ported.
fn window_content(state: &AppState, cx: &mut Cx<AppState>) -> El {
    // React `App.tsx:77-79`: `case 'main': default: return <MainPage />`.
    main_page(state, cx)
}
