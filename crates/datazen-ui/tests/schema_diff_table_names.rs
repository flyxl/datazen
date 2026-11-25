//! Bridge to `views::schema_diff::schema_diff_table_names`'s inline tests
//! (the T351 seam module of the discovery run; T374 owns the full React
//! file): same pure-logic convention as the object-identity bridge.

pub(crate) mod tests {
    pub(crate) use datazen_ui::views::schema_diff::schema_diff_table_names::tests::*;
}
