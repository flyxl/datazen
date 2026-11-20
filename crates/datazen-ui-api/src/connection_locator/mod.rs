mod locate;
mod types;

#[cfg(test)]
mod tests;

pub use locate::{
    connection_expand_key, group_connections_with_recent_sections, normalize,
    order_connections_for_display, parse_connection_expand_key, rank_connections,
};
pub use types::{
    ActiveConnectionSlice, ActiveStatus, Candidate, ConnectionLocatorResult,
    ConnectionLocatorUsageState, ConnectionMatch, ConnectionMatchReason, ConnectionSection,
    ExpandKey, SchemaSlice, PINNED_GROUP_KEY, RECENT_CONNECTION_LIMIT, RECENT_GROUP_KEY,
};
