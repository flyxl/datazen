pub mod mock_driver;

#[cfg(any(test, feature = "test-harness"))]
pub mod app_state;

#[cfg(test)]
pub mod mock_ai_provider;

#[cfg(test)]
pub mod ai_wiremock;

/// Restores prior `DATAZEN_KEYRING` when dropped (avoids cross-test pollution).
pub struct FileKeyringGuard {
    previous: Option<String>,
}

impl FileKeyringGuard {
    pub fn set() -> Self {
        let previous = std::env::var("DATAZEN_KEYRING").ok();
        std::env::set_var("DATAZEN_KEYRING", "file");
        Self { previous }
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
