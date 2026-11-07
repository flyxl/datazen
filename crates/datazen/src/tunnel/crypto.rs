//! Process-wide rustls backend selection — the implementation.
//!
//! The **why** (which provider, why it has to be the first installer, what it
//! races against) is documented where the decision is observed from: the host
//! module `src-tauri/src/tls.rs`. The decision itself lives here because the
//! TLS legs of this crate — `http_proxy`'s `https://` proxy and `websocket`'s
//! `wss://` relay — build an **auto-detecting** `ClientConfig`, and
//! auto-detection panics when both provider features are compiled in and no
//! process default exists.
//!
//! The host re-exports this function unchanged (`datazen::install_default_crypto_provider`),
//! so `main()` still installs it before anything else runs.

use std::sync::OnceLock;

/// Empty by design: the cell exists only to run the decision once per process,
/// and the outcome of that decision is reported through `tracing` at the time.
static PROVIDER: OnceLock<()> = OnceLock::new();

/// Install the process-wide rustls `CryptoProvider` if none is installed yet.
///
/// `aws-lc-rs` is the choice, and it is a choice rather than a fallback:
///
/// - it is rustls 0.23's **own** preferred provider, i.e. the one
///   `ClientConfig::builder()` would have picked on its own had only a single
///   provider feature been enabled, so this reproduces the intended behaviour
///   instead of overriding it;
/// - `tokio-rustls`'s default features and `reqwest`'s `rustls` feature both
///   enable `aws-lc-rs`, and those are the dominant TLS consumers here —
///   `reqwest` carries every AI provider and the HTTP-based drivers, and it
///   *adopts* the process default whenever one exists
///   (`reqwest-0.13.4/src/async_impl/client.rs:719-721`) rather than pinning
///   its own, so choosing `aws-lc-rs` here is what makes the process agree
///   with itself instead of holding two views of one slot.
///
/// ## Concurrency
///
/// `OnceLock::get_or_init` runs the closure at most once per process, blocks
/// every other caller until that one returns, and is **not** poisoned if the
/// closure unwinds — a later call simply retries. `std::sync::Once` would be
/// the wrong primitive here precisely because it *does* poison, turning a
/// single unlucky unwind into a permanent "Once instance has previously been
/// poisoned" panic on every subsequent call.
///
/// The closure performs no check-then-act: it calls `install_default()`
/// unconditionally and lets rustls' own compare-and-swap decide the winner.
/// That removes the time-of-check/time-of-use window a
/// `if get_default().is_none()` guard would leave between the read and the
/// install, and it is why N threads racing here can never panic: exactly one
/// gets `Ok(())`, the rest get the losing provider back as `Err` and the
/// process keeps a working default either way.
///
/// ## Ordering
///
/// Because `tauri-plugin-updater` competes for the same slot with a *different*
/// provider, being first is the whole defence. `main()` calls this before it
/// parses argv, so both entry points (GUI and `--mcp-stdio`) have it installed
/// before the Tauri builder — and therefore before the updater plugin — can run.
/// The tunnel construction sites call it again so that library/test embedders
/// that never go through `main()` are covered too.
///
/// None of that ordering rests on the sentence above. The updater installs
/// nothing from `Builder::build()` or from the plugin's `init()`: its `ring`
/// install sits inside `Updater::check()`
/// (`tauri-plugin-updater-2.10.1/src/updater.rs:446-448`), which the WebView
/// only reaches from `checkForUpdates()` — a user action, or the opt-in
/// `check_for_updates_on_startup` setting — long after `main()` and after the
/// builder has run. `tauri` carries a third copy of the same snippet at
/// `tauri-2.11.5/src/protocol/tauri.rs:44`, but it is `#[cfg(all(dev, mobile))]`
/// and is not compiled into a desktop build. `main()` going first is the
/// cheapest of these three guarantees, and it is the one that still holds if
/// either dependency moves.
///
/// Never panics. Note the `warn!` on the losing branch is only *observable*
/// once a `tracing_subscriber` is installed; when `main()` makes the call that
/// happens before logging is up, so in the shipped app the losing case is
/// silent and the signal exists for embedders that call this later.
pub fn install_default_crypto_provider() {
    PROVIDER.get_or_init(|| {
        // `let _ =` / `match` rather than `.expect(..)`: losing this race is a
        // normal, recoverable outcome, not a bug, and this function is on the
        // per-connection path of every tunnel.
        match tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default() {
            Ok(()) => {
                tracing::debug!("rustls CryptoProvider installed: aws-lc-rs");
            }
            Err(_) => {
                tracing::warn!(
                    "another crate installed a rustls CryptoProvider before \
                     datazen::install_default_crypto_provider(); keeping theirs"
                );
            }
        }
    });
}
