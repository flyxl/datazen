use super::{extract_mysql_view_metadata, mysql_show_view_result, mysql_view_catalog_result};

#[test]
fn mysql_view_body_normalizes_exact_local_database_qualifiers_on_columns_and_relations() {
    let catalog = mysql_view_catalog_result(
        "SELECT source_db.child.id, source_db.child.state, source_db.child.parent_id FROM source_db.child",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT child.id, child.state, child.parent_id FROM child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    assert!(extract_mysql_view_metadata(&catalog, &show).is_ok());
}

#[test]
fn mysql_view_body_normalizes_local_database_qualifier_on_qualified_wildcard() {
    let catalog = mysql_view_catalog_result(
        "SELECT source_db.child.* FROM source_db.child",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT child.* FROM child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    assert!(extract_mysql_view_metadata(&catalog, &show).is_ok());

    let external_catalog = mysql_view_catalog_result(
        "SELECT archive_db.child.* FROM archive_db.child",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let external_changed_to_local = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT child.* FROM child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&external_catalog, &external_changed_to_local).is_err());
}

#[test]
fn mysql_view_body_preserves_external_database_column_and_relation_identity() {
    let external_catalog = mysql_view_catalog_result(
        "SELECT archive_db.child.id FROM archive_db.child",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let same_external_show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT archive_db.child.id FROM archive_db.child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&external_catalog, &same_external_show).is_ok());

    let external_changed_to_local = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT child.id FROM child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&external_catalog, &external_changed_to_local).is_err());

    let two_part_column_catalog = mysql_view_catalog_result(
        "SELECT source_db.id FROM child",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let local_column_show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT id FROM child",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&two_part_column_catalog, &local_column_show).is_err());
}
