//! Errors from portable feature services. Hosts map these to their transport envelope.
#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
    #[error(transparent)]
    Ai(#[from] datazen_ai_api::AiError),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Driver(#[from] datazen_driver_api::DriverError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
impl From<String> for FeatureError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}
