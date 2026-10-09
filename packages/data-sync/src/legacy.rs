//! Retired overwrite-copy path (DROP + CREATE + INSERT). Not Data Synchronization.

use super::error::DataSyncError;

pub const OVERWRITE_COPY_RETIRED: &str = "overwrite copy is no longer Data Synchronization; \
use Diff Sync (identical structure and primary key) or Data Transfer";

pub fn refuse_overwrite_copy() -> DataSyncError {
    DataSyncError::validation(OVERWRITE_COPY_RETIRED)
}

pub fn is_overwrite_copy_retired_message(msg: &str) -> bool {
    msg.contains("overwrite copy is no longer Data Synchronization")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuse_message_is_stable_and_detectable() {
        let err = refuse_overwrite_copy();
        let msg = err.to_string();
        assert!(is_overwrite_copy_retired_message(&msg));
        assert!(msg.contains("Transfer"));
        assert!(msg.contains("Diff Sync"));
        assert_eq!(msg, OVERWRITE_COPY_RETIRED);
    }
}
