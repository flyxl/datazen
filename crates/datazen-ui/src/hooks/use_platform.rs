//! Port of `packages/frontend/src/hooks/usePlatform.ts` (T384) — the
//! OS detection hook (form-split F6 prep, moved home from
//! `src/platform/desktop/os.ts`).
//!
//! The OS dimension (macos/windows/linux) is orthogonal to the form
//! factor — a web build still distinguishes mac/windows browsers. The
//! detection ladder: the host probe (`PlatformServices.getOsPlatform`
//! — the desktop binding's plugin; the web binding and the
//! unconfigured fallback: the user agent), else the user-agent parse.
//! The module cache (`cachedPlatform`) makes the probe a once-per-
//! process concern.
//!
//! ## What maps where
//!
//! - `platformFromUserAgent` is [`platform_from_user_agent`], verbatim
//!   (mac → macos, win → windows, linux → linux, else unknown) — the
//!   host passes the user agent string it saw; the engine sniffs
//!   nothing.
//! - `getPlatformSync` is [`UsePlatformState::platform_sync`]: the
//!   cached answer when the probe already ran, else the user-agent
//!   parse — the synchronous FIRST frame (React's ruling: the first
//!   frame on Windows renders the Windows titlebar instead of briefly
//!   mounting the macOS drag overlay that can intercept WebView2
//!   clicks).
//! - `detectPlatform` + the mount effect is
//!   [`UsePlatformState::begin_detect`] → the host's probe →
//!   [`UsePlatformState::note_probed`]: the probe answer caches and
//!   replaces; a failed probe (`catch(() => null)`) falls to the
//!   user-agent parse and caches THAT.
//! - `usePlatform()`'s read is [`UsePlatformState::platform`].
//! - The [`Platform`] enum moves home from
//!   `sql_editor_context_menu`'s carry (T155's pre-T384 hold); the
//!   re-export there keeps its path stable (the `lib_helpers`
//!   absorption precedent).
//!
//! Known deltas: none behavioural — the ladder and the cache are the
//! React file's verbatim; the probe is the documented host seam (the
//! engine has no platform services to ask).

// No production consumer yet beyond the carried re-export (the host
// track wires the probe); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

/// React `Platform` — the OS dimension, orthogonal to the form factor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Windows,
    Linux,
    #[default]
    Unknown,
}

/// `platformFromUserAgent`: mac → macos, win → windows, linux →
/// linux, else unknown (lowercased substring checks, the order
/// verbatim).
pub fn platform_from_user_agent(user_agent: &str) -> Platform {
    let ua = user_agent.to_lowercase();
    if ua.contains("mac") {
        Platform::Macos
    } else if ua.contains("win") {
        Platform::Windows
    } else if ua.contains("linux") {
        Platform::Linux
    } else {
        Platform::Unknown
    }
}

/// The hook's state: the synchronous first frame plus the probe's
/// cached answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsePlatformState {
    /// The current answer: the user-agent parse until the probe
    /// answers, the cached probe answer after (React's `platform`
    /// state).
    platform: Platform,
    /// `cachedPlatform`: once the probe answered, every later hook
    /// instance (and every `getPlatformSync`) reads the cache.
    cached: Option<Platform>,
}

impl UsePlatformState {
    /// React's mount: the synchronous user-agent seed (the first
    /// frame paints with it — the WebView2 ruling).
    pub fn mount(&mut self, user_agent: &str) {
        self.platform = self.cached.unwrap_or_else(|| platform_from_user_agent(user_agent));
    }

    /// The hook's read.
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// `getPlatformSync()`: the cached answer when the probe already
    /// ran, else the user-agent parse (this read never consults the
    /// probe).
    pub fn platform_sync(&self, user_agent: &str) -> Platform {
        self.cached.unwrap_or_else(|| platform_from_user_agent(user_agent))
    }

    /// The mount effect's probe: whether the host should run
    /// `getOsPlatform` (false when the cache already holds — the
    /// `if (cachedPlatform) return cachedPlatform` fast path).
    pub fn begin_detect(&self) -> bool {
        self.cached.is_none()
    }

    /// The probe answered (a failure is `None`: the user-agent parse
    /// caches in its place — `probed ?? platformFromUserAgent()`).
    pub fn note_probed(&mut self, probed: Option<Platform>, user_agent: &str) {
        let answer = probed.unwrap_or_else(|| platform_from_user_agent(user_agent));
        self.cached = Some(answer);
        self.platform = answer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_parse_is_verbatim() {
        assert_eq!(platform_from_user_agent("Mozilla/5.0 (Macintosh)"), Platform::Macos);
        assert_eq!(platform_from_user_agent("Mozilla/5.0 (Windows NT 10.0)"), Platform::Windows);
        assert_eq!(platform_from_user_agent("Mozilla/5.0 (X11; Linux x86_64)"), Platform::Linux);
        assert_eq!(platform_from_user_agent("Mozilla/5.0 (iPhone)"), Platform::Unknown);
        assert_eq!(platform_from_user_agent(""), Platform::Unknown);
        // Lowercased before the checks.
        assert_eq!(platform_from_user_agent("MACINTOSH"), Platform::Macos);
    }

    #[test]
    fn the_first_frame_seeds_from_the_user_agent() {
        let mut state = UsePlatformState::default();
        state.mount("Mozilla/5.0 (Windows NT 10.0)");
        assert_eq!(state.platform(), Platform::Windows);
        // A mount over an existing cache reads the cache (the probe's
        // answer wins across hook instances).
        state.note_probed(Some(Platform::Macos), "Mozilla/5.0 (Windows)");
        state.mount("Mozilla/5.0 (Windows)");
        assert_eq!(state.platform(), Platform::Macos, "the cache beats the agent");
    }

    #[test]
    fn the_probe_caches_and_the_sync_read_honors_it() {
        let mut state = UsePlatformState::default();
        state.mount("Mozilla/5.0 (Linux x86_64)");
        // The probe has not run: sync reads the parse.
        assert!(state.begin_detect(), "the mount effect probes");
        assert_eq!(state.platform_sync("Mozilla/5.0 (Linux x86_64)"), Platform::Linux);
        // The probe answers macOS: the state AND the cache move.
        state.note_probed(Some(Platform::Macos), "Mozilla/5.0 (Linux)");
        assert_eq!(state.platform(), Platform::Macos);
        assert_eq!(state.platform_sync("anything"), Platform::Macos, "sync reads the cache");
        // A second mount never probes again.
        assert!(!state.begin_detect(), "the cache short-circuits");
        // A FAILED probe caches the user-agent parse.
        let mut failed = UsePlatformState::default();
        failed.mount("Mozilla/5.0 (Windows NT)");
        failed.note_probed(None, "Mozilla/5.0 (Windows NT)");
        assert_eq!(failed.platform(), Platform::Windows);
        assert_eq!(failed.platform_sync("Mozilla/5.0 (Macintosh)"), Platform::Windows);
        assert!(!failed.begin_detect(), "the fallback caches too");
    }
}
