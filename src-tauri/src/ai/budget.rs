//! Token budget constants for AI / MCP schema context injection.

/// Budget for pinned-table DDL in the schema pipeline.
pub const PINNED_DDL: usize = 4000;
/// Budget when falling back to a broader schema DDL scan.
pub const FALLBACK_DDL: usize = 4000;
/// Budget for diagnose-error schema context.
pub const DIAGNOSE: usize = 3000;
/// Budget for schema-documentation selective context.
pub const SCHEMA_DOC: usize = 8000;
/// Budget for MCP schema resource reads.
pub const MCP_RESOURCE: usize = 8000;
