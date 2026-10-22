//! The keyset-paging half of the driver contract.
//!
//! Keyset (seek) pagination reads the page *after a value boundary* instead
//! of after a number of skipped rows. It is the answer to a deep page being
//! slow, but it only works when the page boundary is a **fact about the data**
//! rather than a guess, and only when the dialect can express the boundary as
//! a condition.
//!
//! Two questions decide that, and they belong to two different layers:
//!
//! * **Does the order make a boundary meaningful?** The rows of two pages
//!   must never compare equal, or the engine is free to order them either way
//!   and a boundary stops meaning anything. That is a question about the
//!   *table* (does it have a key?), so the host answers it — see
//!   `datazen_feature_core::services::table_paging`.
//! * **Can this dialect express the boundary?** That is a question about the
//!   *dialect*, so the driver answers it, in
//!   [`keyset_predicate_sql`].
//!
//! The defaults here are chosen so that a driver that writes nothing behaves
//! exactly as it did before this contract existed:
//!
//! * `supports_keyset_pagination` is `false`, so the host never builds a
//!   cursor and every read keeps its `OFFSET`;
//! * `keyset_predicate_sql` is `None`, so a driver that never opted in is
//!   never asked for a condition.
//!
//! A driver that opts in gets the portable expansion for free:
//! [`crate::keyset::lexicographic_seek_predicate`] renders the
//! lexicographic "after this boundary" condition in the dialect's own
//! identifier quoting and literal syntax. A dialect that supports row-value
//! comparison may write its own, shorter form.

use crate::traits::DatabaseDriver;
use crate::types::Value;

/// Whether this driver can read a page by seek.
///
/// The default is `false`: the host then reads every page with an `OFFSET`,
/// exactly as it did before this method existed. A driver that answers `true`
/// also has to answer [`keyset_predicate_sql`], because a driver that claims
/// the capability but cannot render a condition would leave the host with
/// nothing to seek by.
pub fn supports_keyset_pagination<D: DatabaseDriver + ?Sized>(_driver: &D) -> bool {
    false
}

/// The `WHERE` fragment that reads the rows after a boundary.
///
/// The fragment is a complete piece of SQL **without** a leading `WHERE` or
/// `AND`; the host parenthesises it and joins it with `AND` to the filters
/// that are already there. Values are inlined by the driver's own literal
/// formatter, because the browsing read path binds no parameters.
///
/// `order_columns` is the `ORDER BY` in order, `true` meaning `DESC`, and
/// `boundary` holds one value per column in the same order. `None` means the
/// combination cannot be turned into a safe condition — the host then falls
/// back to `OFFSET` paging for this request and says so, rather than sending
/// a condition that would skip or repeat rows.
///
/// The default is `None`: a driver that did not opt in is never asked.
pub fn keyset_predicate_sql<D: DatabaseDriver + ?Sized>(
    _driver: &D,
    _order_columns: &[(String, bool)],
    _boundary: &[(String, Value)],
) -> Option<String> {
    None
}
