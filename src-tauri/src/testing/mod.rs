pub use datazen_driver_api::mock_driver;

#[cfg(any(test, feature = "test-harness"))]
pub mod app_state;

#[cfg(any(test, feature = "test-harness"))]
pub(crate) mod resource_provider;

#[cfg(test)]
pub mod mock_ai_provider;

#[cfg(test)]
pub mod ai_wiremock;

/// Process-wide lock for every test that touches the process environment.
///
/// `std::env::set_var` / `remove_var` are **process-global**: a writer in one
/// module races with a reader in another, and the two are usually in different
/// modules (the keyring backend lives in `store::key_backend`, the opt-in
/// switch next to it, the store fixtures in `store::tests::fixtures`). Every
/// env writer **and** env reader in the test binary takes this one mutex, so
/// there is a single serialization point instead of several per-module ones.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Forces `DATAZEN_KEYRING=keyring` and restores the previous value on drop.
///
/// Only the `#[ignore]`d keychain tests use this: they run on their own
/// (`cargo test -- --ignored`), never next to the rest of the suite, so the
/// process env may be mutated there. The previous value is **restored**
/// (not overwritten with `file`), so the guard cannot leak a backend into a
/// later test of the same run.
#[cfg(test)]
pub(crate) struct ForceKeyringGuard {
    previous: Option<String>,
}

#[cfg(test)]
impl ForceKeyringGuard {
    pub(crate) fn new() -> Self {
        let previous = std::env::var("DATAZEN_KEYRING").ok();
        std::env::set_var("DATAZEN_KEYRING", "keyring");
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for ForceKeyringGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var("DATAZEN_KEYRING", v),
            None => std::env::remove_var("DATAZEN_KEYRING"),
        }
    }
}
