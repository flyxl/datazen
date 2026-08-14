pub mod mock_driver;

#[cfg(test)]
pub mod app_state;

#[cfg(test)]
pub mod mock_ai_provider;

#[cfg(test)]
pub mod ai_wiremock;

use std::sync::{Arc, OnceLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

fn test_keyring_semaphore() -> Arc<Semaphore> {
    static SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
}

/// Keeps the process-wide file-keyring environment isolated for command tests.
///
/// `DATAZEN_KEYRING` is process-global, so `TestAppState` instances cannot be
/// initialized or dropped concurrently without allowing one test to overwrite
/// another test's environment. The permit is held for the lifetime of the
/// `TestAppState` that owns this guard.
pub struct FileKeyringGuard {
    previous: Option<String>,
    _permit: OwnedSemaphorePermit,
}

impl FileKeyringGuard {
    pub async fn set() -> Self {
        let permit = test_keyring_semaphore()
            .acquire_owned()
            .await
            .expect("test keyring semaphore closed");
        let previous = std::env::var("DATAZEN_KEYRING").ok();
        std::env::set_var("DATAZEN_KEYRING", "file");
        Self {
            previous,
            _permit: permit,
        }
    }
}

impl Drop for FileKeyringGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var("DATAZEN_KEYRING", v),
            None => std::env::remove_var("DATAZEN_KEYRING"),
        }
    }
}
