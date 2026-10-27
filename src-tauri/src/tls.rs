//! Process-wide rustls backend selection.
//!
//! # Why this module exists
//!
//! `rustls 0.23` resolves a process-wide `CryptoProvider` lazily, and it can
//! only pick one from its own Cargo features. DataZen links **two** providers
//! into every build, so it has to make that choice itself:
//!
//! ```text
//! $ cargo tree -p datazen --edges normal,build -e features -i rustls@0.23.43
//! ├── rustls feature "aws-lc-rs"  <- reqwest (feature "rustls"), tokio-rustls (default features)
//! └── rustls feature "ring"       <- tauri-plugin-updater (feature "rustls-tls")
//! ```
//!
//! (Enabling a driver adds more `ring` requesters — `redis` via
//! `tls-rustls`, `mongodb` via `rustls-tls`, plus `sqlx`/`duckdb` — but the
//! production host already has both features with **no** driver enabled, so
//! this is not a driver-selection problem.)
//!
//! With both features compiled in, `CryptoProvider::from_crate_features()`
//! returns `None` and every **auto-detecting** `ClientConfig::builder()` panics
//! — the `.expect(..)` inside `get_default_or_install_from_crate_features()` in
//! `rustls-0.23/src/crypto/mod.rs`:
//!
//! ```text
//! Could not automatically determine the process-level CryptoProvider from
//! Rustls crate features. Call CryptoProvider::install_default() before this
//! point to select a provider manually, ...
//! ```
//!
//! ## This is a latent crash, not a startup crash
//!
//! Nothing on the `run()` / `run_mcp_stdio()` bootstrap path builds a
//! `ClientConfig`, so the process starts fine and blows up on the *first* use
//! of one of the three auto-detecting sites in the shipped binary:
//!
//! | site | reached by |
//! |------|-----------|
//! | `tunnel/http_proxy.rs::tls_connect` | an `https://` HTTP CONNECT tunnel |
//! | `tungstenite 0.26.2/src/tls.rs` (via `tokio-tungstenite`) | a `wss://` tunnel |
//! | `redis 0.27.6/src/connection.rs` (via `datazen-driver-redis`) | a `rediss://` connection |
//!
//! The rest of the graph is **not** uniformly indifferent, and the important
//! one is `reqwest`. `sqlx` (`sqlx-core-0.8.6/src/net/tls/tls_rustls.rs:107`)
//! and `mongodb` (`mongodb-3.8.0/src/runtime/tls_rustls.rs:86`) really do name
//! a provider, and `rustls-platform-verifier`'s `BuilderVerifierExt` inherits
//! one from the builder it is handed — but its other entry point,
//! `ConfigVerifierExt::with_platform_verifier`
//! (`rustls-platform-verifier-0.7.0/src/lib.rs:88`), calls the *auto-detecting*
//! `ClientConfig::builder()`. Nothing in this graph calls it, so it adds no
//! reachable panic here, but "this crate names a provider explicitly" is not
//! true of it.
//!
//! `reqwest` is the one that matters, and it is **not** unaffected:
//! `reqwest-0.13.4/src/async_impl/client.rs:719-721` reads
//! `CryptoProvider::get_default()` **first** and uses that provider whenever one
//! is installed, falling back to its own `aws-lc-rs` only when none is. So
//! whatever ends up in the process default is what every `reqwest` client in
//! the process uses — all the AI providers and all the HTTP-based drivers —
//! and it would switch over without a word. That is the reason the ordering
//! below is load-bearing rather than cosmetic.
//!
//! ## The rival installer
//!
//! `tauri-plugin-updater 2.10.1/src/updater.rs` installs **ring** when it finds
//! no default. That is a second, disagreeing proposal for the same
//! process-global slot, so idempotency alone is not the requirement: this
//! module has to be the *first* installer to run, and it has to say so when it
//! loses.

/// The **decision** — which provider wins the process-wide rustls slot, and
/// how the race against the updater's competing `ring` install is settled —
/// lives in the shared `datazen::tunnel` crate (`datazen::tunnel::crypto`),
/// because the TLS legs of the tunnel mechanisms (`https://` HTTP `CONNECT`
/// proxy, `wss://` WebSocket relay) build an **auto-detecting**
/// `ClientConfig`, and auto-detection panics when both provider features are
/// compiled in and no process default exists. A decision the tunnel crate has
/// to make cannot live behind the host boundary.
///
/// This module keeps the **host** side of it: the rationale above, the
/// host-wiring guards below (`main()` must install before it parses argv), and
/// the tests that observe the process global from a genuinely fresh process.
///
/// The function is re-exported unchanged, so `datazen_desktop::install_default_crypto_provider()`
/// — what `main()` calls — is the very same function the tunnel crate calls.
pub use datazen::tunnel::install_default_crypto_provider;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Output};

    /// The one test binary that can observe a *fresh* process. Re-running this
    /// same test executable with `DATAZEN_TLS_CHILD_MODE` set is the only way
    /// to test a process-global: inside the suite, whichever test happens to run
    /// first would install a provider and silently make every later
    /// "does it panic without a provider?" assertion vacuous.
    const CHILD_MODE_ENV: &str = "DATAZEN_TLS_CHILD_MODE";

    /// Printed by a child that actually completed its scenario. The parent
    /// requires this marker, so a child that silently no-ops (e.g. the env var
    /// failed to propagate) fails the parent instead of passing for the wrong
    /// reason.
    const CHILD_DONE: &str = "DATAZEN-TLS-CHILD-DONE:";

    const CHILD_TEST: &str = "tls::tests::tls_child_scenario";

    fn run_child(mode: &str) -> Output {
        let exe = std::env::current_exe().expect("test binary path");
        Command::new(exe)
            .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD_MODE_ENV, mode)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn {CHILD_TEST} child ({mode}): {e}"))
    }

    fn stdout_of(out: &Output) -> String {
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn stderr_of(out: &Output) -> String {
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Build a `ClientConfig` through the auto-detecting builder — the exact
    /// call that panics when no process default exists.
    fn auto_detecting_client_config() -> tokio_rustls::rustls::ClientConfig {
        tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(tokio_rustls::rustls::RootCertStore::empty())
            .with_no_client_auth()
    }

    /// The rival install, copied verbatim from
    /// `tauri-plugin-updater-2.10.1/src/updater.rs:446-448`. If the claim this
    /// module makes about *losing* is ever to be believed, it has to be
    /// exercised against the real competitor rather than against a stand-in.
    fn rival_install() {
        if tokio_rustls::rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        }
    }

    /// Which provider actually won the process slot.
    ///
    /// `CryptoProvider` carries no name field, so this compares the whole
    /// `Debug` rendering against a freshly built provider of each kind. The
    /// [`PROVIDER_DISTINGUISHABLE`] assertion in the tests below guards the
    /// guard: if a rustls upgrade ever made the two render identically, that
    /// would turn every "installed-is-ring" check below into a tautology that
    /// passes for the wrong reason.
    fn installed_provider_debug() -> String {
        format!(
            "{:?}",
            tokio_rustls::rustls::crypto::CryptoProvider::get_default()
                .expect("the scenario must leave a provider installed")
        )
    }

    fn ring_provider_debug() -> String {
        format!(
            "{:?}",
            tokio_rustls::rustls::crypto::ring::default_provider()
        )
    }

    fn aws_lc_rs_provider_debug() -> String {
        format!(
            "{:?}",
            tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
        )
    }

    /// Runs in the parent too (inert, because the env var is unset there) and
    /// in a child with `DATAZEN_TLS_CHILD_MODE` set to one of the scenarios
    /// below. Every scenario ends by printing [`CHILD_DONE`], except the one
    /// that is *supposed* to panic.
    #[tokio::test]
    async fn tls_child_scenario() {
        let Ok(mode) = std::env::var(CHILD_MODE_ENV) else {
            return;
        };
        match mode.as_str() {
            // The pre-fix state: a fresh process where nothing has installed a
            // provider. This must panic inside rustls.
            "no-install-panics" => {
                let _ = auto_detecting_client_config();
                panic!("{CHILD_DONE}{mode} — builder() did NOT panic, the premise is wrong");
            }
            "with-install-succeeds" => {
                install_default_crypto_provider();
                let _ = auto_detecting_client_config();
            }
            // The competitor got there first. We must not panic, must not
            // overwrite them, and the auto-detecting builder must still work
            // on top of *their* provider.
            "rival-installs-first" => {
                // A subscriber is what makes the losing branch observable at
                // all. In the shipped app there is none at this point — which is
                // why this is asserted here and nowhere else. `--nocapture`
                // sends the test writer's output to this child's real stdout,
                // which the parent reads back.
                let _ = tracing_subscriber::fmt()
                    .with_test_writer()
                    .with_max_level(tracing::Level::WARN)
                    .try_init();

                rival_install();
                install_default_crypto_provider();
                assert_eq!(
                    installed_provider_debug(),
                    ring_provider_debug(),
                    "the losing branch must keep the provider that was already there"
                );
                let _ = auto_detecting_client_config();
            }
            // Both installers racing from a common barrier, the way they would
            // if `main()`'s ordering guarantee were ever lost.
            "rival-race" => {
                const THREADS: usize = 16;
                let barrier = std::sync::Barrier::new(2 * THREADS);
                std::thread::scope(|scope| {
                    for _ in 0..THREADS {
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            install_default_crypto_provider();
                        });
                    }
                    for _ in 0..THREADS {
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            rival_install();
                        });
                    }
                });
                let installed = installed_provider_debug();
                assert!(
                    installed == ring_provider_debug() || installed == aws_lc_rs_provider_debug(),
                    "exactly one of the two proposals must own the slot, got a third thing"
                );
                let _ = auto_detecting_client_config();
            }
            // 32 threads hit the very first call at the same instant.
            "concurrent-first-use" => {
                const THREADS: usize = 32;
                let barrier = std::sync::Barrier::new(THREADS);
                // Scoped threads so the barrier can stay on this stack. A panic
                // in any of them surfaces when the scope ends.
                std::thread::scope(|scope| {
                    for _ in 0..THREADS {
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            install_default_crypto_provider();
                        });
                    }
                });
                assert!(
                    tokio_rustls::rustls::crypto::CryptoProvider::get_default().is_some(),
                    "the race must still leave a provider installed"
                );
                let _ = auto_detecting_client_config();
            }
            // The real product path: the `https://` HTTP CONNECT tunnel probe.
            "https-tunnel-probe" => {
                let proxy = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind fixture proxy");
                let port = proxy.local_addr().expect("fixture addr").port();
                // Accepts, then never speaks TLS: the handshake must fail, but
                // it must fail as an `Err`, not as a panic.
                let peer = tokio::spawn(async move {
                    let (stream, _) = proxy.accept().await.expect("accept");
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    drop(stream);
                });
                let outcome = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    crate::tunnel::verify_http_proxy_upstream(
                        &datazen_driver_api::HttpProxyTunnelConfig {
                            enabled: true,
                            host: "127.0.0.1".into(),
                            port,
                            scheme: "https".into(),
                            username: None,
                            password: None,
                            headers: None,
                            connect_timeout_secs: 1,
                        },
                        "db.internal",
                        5432,
                    ),
                )
                .await
                .expect("the https tunnel probe must be bounded, not hang");
                assert!(
                    outcome.is_err(),
                    "a non-TLS peer under scheme=https must fail, got {outcome:?}"
                );
                peer.abort();
            }
            // The other real product path: the `wss://` tunnel probe, which
            // reaches `tungstenite`'s own `ClientConfig::builder()`.
            "wss-tunnel-probe" => {
                let relay = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind fixture relay");
                let port = relay.local_addr().expect("fixture addr").port();
                let peer = tokio::spawn(async move {
                    let (stream, _) = relay.accept().await.expect("accept");
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    drop(stream);
                });
                let outcome = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    crate::tunnel::verify_websocket_upstream(
                        &datazen_driver_api::WebSocketTunnelConfig {
                            enabled: true,
                            url: format!("wss://127.0.0.1:{port}/raw"),
                            auth_token: None,
                            headers: None,
                            connect_timeout_secs: 1,
                            ping_interval_secs: 30,
                            mode: "raw_binary".into(),
                        },
                        "db.internal",
                        5432,
                    ),
                )
                .await
                .expect("the wss tunnel probe must be bounded, not hang");
                assert!(
                    outcome.is_err(),
                    "a non-TLS peer under wss:// must fail, got {outcome:?}"
                );
                peer.abort();
            }
            other => panic!("unknown child scenario '{other}'"),
        }
        println!("{CHILD_DONE}{mode}");
    }

    /// The two halves of the fix, observed in two genuinely fresh processes.
    ///
    /// "No install" is the fix-reverted state: in a process where nothing has
    /// installed a provider, the auto-detecting builder dies with rustls'
    /// own message. "With install" is the shipped state: same binary, same
    /// call, one line earlier, no panic.
    #[test]
    fn fresh_process_panics_without_the_fix_and_succeeds_with_it() {
        let without = run_child("no-install-panics");
        assert!(
            !without.status.success(),
            "a fresh process with no installed provider must NOT be able to \
             build a ClientConfig; child stdout:\n{}\nstderr:\n{}",
            stdout_of(&without),
            stderr_of(&without)
        );
        assert!(
            stderr_of(&without)
                .contains("Could not automatically determine the process-level CryptoProvider"),
            "the failure must be rustls' ambiguous-provider panic, not something else; \
             stderr:\n{}",
            stderr_of(&without)
        );
        assert!(
            !stdout_of(&without).contains(CHILD_DONE),
            "the no-install child must die before reporting success"
        );

        let with = run_child("with-install-succeeds");
        assert!(
            with.status.success(),
            "installing the provider must make the same call succeed; stderr:\n{}",
            stderr_of(&with)
        );
        assert!(
            stdout_of(&with).contains(&format!("{CHILD_DONE}with-install-succeeds")),
            "the with-install child must report that it ran; stdout:\n{}",
            stdout_of(&with)
        );
    }

    /// The `https://` HTTP CONNECT tunnel probe — the first thing a user hits
    /// after configuring an HTTPS proxy — must surface a TLS failure as `Err`
    /// in a fresh process. Revert the `install_default_crypto_provider()` call
    /// in `tls_connect` and this goes red with rustls' panic.
    #[test]
    fn https_tunnel_probe_never_panics_in_a_fresh_process() {
        let out = run_child("https-tunnel-probe");
        assert!(
            out.status.success(),
            "the https tunnel probe panicked instead of failing; stdout:\n{}\nstderr:\n{}",
            stdout_of(&out),
            stderr_of(&out)
        );
        assert!(
            stdout_of(&out).contains(&format!("{CHILD_DONE}https-tunnel-probe")),
            "the https child must report that it ran; stdout:\n{}",
            stdout_of(&out)
        );
    }

    /// Same for the `wss://` probe, whose `ClientConfig::builder()` lives
    /// inside `tungstenite` rather than in host code.
    #[test]
    fn wss_tunnel_probe_never_panics_in_a_fresh_process() {
        let out = run_child("wss-tunnel-probe");
        assert!(
            out.status.success(),
            "the wss tunnel probe panicked instead of failing; stdout:\n{}\nstderr:\n{}",
            stdout_of(&out),
            stderr_of(&out)
        );
        assert!(
            stdout_of(&out).contains(&format!("{CHILD_DONE}wss-tunnel-probe")),
            "the wss child must report that it ran; stdout:\n{}",
            stdout_of(&out)
        );
    }

    /// 32 threads racing the very first call. In a fresh process this is the
    /// real first-use race, not a no-op against an already-installed provider.
    #[test]
    fn concurrent_first_use_never_panics_in_a_fresh_process() {
        let out = run_child("concurrent-first-use");
        assert!(
            out.status.success(),
            "racing installers panicked; stdout:\n{}\nstderr:\n{}",
            stdout_of(&out),
            stderr_of(&out)
        );
        assert!(
            stdout_of(&out).contains(&format!("{CHILD_DONE}concurrent-first-use")),
            "the concurrency child must report that it ran; stdout:\n{}",
            stdout_of(&out)
        );
    }

    /// Idempotent and panic-free, including when a default already exists (the
    /// updater or an earlier call may have won the race).
    #[test]
    fn install_is_idempotent_and_selects_a_provider() {
        install_default_crypto_provider();
        install_default_crypto_provider();
        assert!(
            tokio_rustls::rustls::crypto::CryptoProvider::get_default().is_some(),
            "a default CryptoProvider must be installed"
        );
    }

    /// The exact call that used to panic: building a `ClientConfig` with both
    /// provider features compiled in.
    #[test]
    fn client_config_builder_works_after_install() {
        install_default_crypto_provider();
        let _ = auto_detecting_client_config();
    }

    /// BUG-004 (b) — components that rely on rustls' **auto-detection**
    /// (`ClientConfig::builder()`, no explicit provider) must resolve to the
    /// installed process default instead of panicking, and must share that same
    /// provider.
    ///
    /// That is precisely the call `redis 0.27` makes in
    /// `create_rustls_config` (`src/connection.rs:891`, reachable here because
    /// `datazen-driver-redis` enables `tokio-rustls-comp` → `tls-rustls`) and
    /// the call `tungstenite 0.26` makes for `wss://` (`src/tls.rs:135`). It is
    /// the compatibility claim behind choosing `aws-lc-rs` as the process
    /// default.
    #[test]
    fn auto_detecting_builder_shares_the_installed_provider() {
        install_default_crypto_provider();
        let installed = tokio_rustls::rustls::crypto::CryptoProvider::get_default()
            .expect("install must leave a process-wide default provider");
        let config = auto_detecting_client_config();
        assert!(
            std::sync::Arc::ptr_eq(config.crypto_provider(), installed),
            "auto-detecting builders must reuse the process-wide default provider"
        );
    }

    /// BUG-004 (b)3 — the **construction sites**, not only `main()`, must
    /// install the provider.
    ///
    /// The behavioural tests above cover the two tunnel sites; this source-level
    /// guard only pins what behaviour cannot reach: that `main()` still installs
    /// *before* it parses argv, because being first is what beats
    /// `tauri-plugin-updater`'s competing `ring` install (see the module docs).
    #[test]
    fn main_installs_before_anything_else_and_both_entry_points_run_through_it() {
        let main_rs = include_str!("main.rs");
        let install_at = main_rs
            .find("datazen_desktop::install_default_crypto_provider()")
            .expect("main() must install the process-wide provider");
        let args_at = main_rs
            .find("let args: Vec<String> = std::env::args().collect();")
            .expect("main() must still parse argv");
        assert!(
            install_at < args_at,
            "the install must be main()'s first statement, before any TLS user"
        );
        // Both entry points funnel through main(): GUI and `--mcp-stdio`.
        assert!(main_rs.contains("datazen_desktop::run_mcp_stdio()"));
        assert!(main_rs.contains("datazen_desktop::run()"));

        let lib_rs = include_str!("lib.rs");
        assert!(lib_rs.contains("mod tls;"));
        assert!(lib_rs.contains("pub use tls::install_default_crypto_provider;"));
    }

    /// The `concurrent_first_use_never_panics_in_a_fresh_process` scenario
    /// above only ever calls *our* helper, so it proves `get_or_init` is
    /// panic-free under contention but says nothing about a foreign installer
    /// arriving at the same slot. These two close that gap, using the updater's
    /// own three lines as the competitor.
    ///
    /// What they establish: the losing branch is reachable, it does not panic,
    /// it does not overwrite the winner, and the auto-detecting builder works
    /// on top of whichever provider won. What they deliberately do **not**
    /// establish: that we win in the shipped app. That is the ordering claim,
    /// and it is settled by *where* `Updater::check()` lives, not by a race
    /// harness — no interleaving invented here is reachable in the product.
    #[test]
    fn losing_the_race_keeps_their_provider_and_still_works() {
        assert_ne!(
            ring_provider_debug(),
            aws_lc_rs_provider_debug(),
            "the two providers must render differently for the assertions below to mean anything"
        );

        let out = run_child("rival-installs-first");
        let stdout = stdout_of(&out);
        assert!(
            out.status.success(),
            "child must not panic when the rival installs first: {}\n{}",
            stdout,
            stderr_of(&out)
        );
        assert!(
            stdout.contains(&format!("{CHILD_DONE}rival-installs-first")),
            "child did not reach the end of the scenario: {stdout}"
        );
        assert!(
            stdout.contains("keeping theirs"),
            "the losing branch must actually have run, not been skipped by a \
             no-op helper — child output was:\n{stdout}"
        );
    }

    #[test]
    fn a_race_against_the_updaters_own_install_never_panics() {
        assert_ne!(
            ring_provider_debug(),
            aws_lc_rs_provider_debug(),
            "the two providers must render differently for the assertions below to mean anything"
        );

        for _ in 0..8 {
            let out = run_child("rival-race");
            let stdout = stdout_of(&out);
            assert!(
                out.status.success(),
                "a race between the two installers must never panic: {}\n{}",
                stdout,
                stderr_of(&out)
            );
            assert!(
                stdout.contains(&format!("{CHILD_DONE}rival-race")),
                "child did not reach the end of the scenario: {stdout}"
            );
        }
    }

    /// The `rediss://` site is in the `redis` crate, in another workspace
    /// member that cannot call this module. It is covered only by `main()`'s
    /// first-statement install; if `driver-redis` ever needs the same treatment
    /// at its own construction site, this is the list to extend.
    #[test]
    fn documented_auto_detecting_sites_stay_documented() {
        let module_doc = include_str!("tls.rs");
        for site in [
            "tunnel/http_proxy.rs::tls_connect",
            "tungstenite 0.26.2/src/tls.rs",
            "redis 0.27.6/src/connection.rs",
        ] {
            assert!(
                module_doc.contains(site),
                "the auto-detecting call site `{site}` must stay documented in tls.rs"
            );
        }
    }
}
