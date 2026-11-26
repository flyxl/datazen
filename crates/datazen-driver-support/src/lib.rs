//! SQL text helpers for DataZen drivers. The statement scanner is also used
//! by the SQL editor, and this facade keeps its public driver path stable.

pub use datazen_sql::sql_split;
pub mod sql_identifiers;
