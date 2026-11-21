use super::{
    DATA_SYNC_DB_REQUIRED_KEY, DATA_SYNC_SAME_DB_KEY, DATA_SYNC_SELECT_BOTH_KEY,
    DATA_SYNC_TRANSFER_HINT_KEY,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointInput {
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub is_same_endpoint: bool,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub pairing_supported: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationFailure {
    SelectBoth,
    SameDb,
    DbRequired,
    TransferHint,
}

impl ValidationFailure {
    pub fn key(self) -> &'static str {
        match self {
            Self::SelectBoth => DATA_SYNC_SELECT_BOTH_KEY,
            Self::SameDb => DATA_SYNC_SAME_DB_KEY,
            Self::DbRequired => DATA_SYNC_DB_REQUIRED_KEY,
            Self::TransferHint => DATA_SYNC_TRANSFER_HINT_KEY,
        }
    }
}

pub fn validate_endpoints(input: &EndpointInput) -> Option<ValidationFailure> {
    if input.source_id.as_deref().unwrap_or("").is_empty()
        || input.target_id.as_deref().unwrap_or("").is_empty()
    {
        return Some(ValidationFailure::SelectBoth);
    }
    if input.is_same_endpoint {
        return Some(ValidationFailure::SameDb);
    }
    if input.source_database.as_deref().unwrap_or("").is_empty()
        || input.target_database.as_deref().unwrap_or("").is_empty()
    {
        return Some(ValidationFailure::DbRequired);
    }
    if !input.pairing_supported {
        return Some(ValidationFailure::TransferHint);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> EndpointInput {
        EndpointInput {
            source_id: Some("s".to_owned()),
            target_id: Some("t".to_owned()),
            is_same_endpoint: false,
            source_database: Some("db1".to_owned()),
            target_database: Some("db2".to_owned()),
            pairing_supported: true,
        }
    }

    #[test]
    fn valid_endpoints_pass() {
        assert_eq!(validate_endpoints(&input()), None);
    }

    #[test]
    fn failures_are_reported_in_the_react_order() {
        let mut item = input();
        item.source_id = None;
        assert_eq!(
            validate_endpoints(&item),
            Some(ValidationFailure::SelectBoth)
        );
        let mut item = input();
        item.is_same_endpoint = true;
        assert_eq!(validate_endpoints(&item), Some(ValidationFailure::SameDb));
        let mut item = input();
        item.target_database = None;
        assert_eq!(
            validate_endpoints(&item),
            Some(ValidationFailure::DbRequired)
        );
        let mut item = input();
        item.pairing_supported = false;
        assert_eq!(
            validate_endpoints(&item),
            Some(ValidationFailure::TransferHint)
        );
    }

    #[test]
    fn error_keys_are_pinned() {
        assert_eq!(ValidationFailure::SelectBoth.key(), "sync.selectBoth");
        assert_eq!(ValidationFailure::SameDb.key(), "sync.cannotSameDb");
        assert_eq!(ValidationFailure::DbRequired.key(), "sync.selectDbRequired");
        assert_eq!(
            ValidationFailure::TransferHint.key(),
            "sync.useTransferHint"
        );
    }
}
