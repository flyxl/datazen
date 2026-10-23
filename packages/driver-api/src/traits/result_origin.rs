//! The default answer of the result-origin contract.
//!
//! The host asks every driver where the rows of a result came from. A driver
//! that has not implemented the capability answers "I cannot tell", and the
//! result grid stays read-only — the same behaviour it had before the
//! contract existed.
//!
//! The default therefore lives here rather than in the trait declaration, so
//! that the declaration in `traits.rs` stays a one-liner and the reasoning —
//! which driver authors need — has one home.

use crate::result_origin::{ResultOrigin, UndeterminableReason};
use crate::types::{ConnectionHandle, DriverError};
use crate::{DatabaseDriver, SqlTarget};

/// The default verdict: the driver does not implement the capability.
///
/// Note what this is **not**: it is not "the result is read-only because the
/// driver says so". It is "nobody has told the host where these rows came
/// from", which the host treats as read-only as well — the two differ only in
/// the reason the user is shown.
pub(crate) async fn describe_result_origin<D>(
    _driver: &D,
    _handle: &ConnectionHandle,
    _target: SqlTarget<'_>,
    _result_columns: &[String],
    _sql: &str,
) -> Result<ResultOrigin, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    Ok(ResultOrigin::Undeterminable(
        UndeterminableReason::NotImplemented,
    ))
}
