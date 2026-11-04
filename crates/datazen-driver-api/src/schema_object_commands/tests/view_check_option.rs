use super::{extract_mysql_view_metadata, mysql_show_view_result, mysql_view_catalog_result};

#[test]
fn mysql_view_metadata_accepts_plain_trailing_check_option_suffix() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "CASCADED",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 WITH CASCADED CHECK OPTION",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    let metadata = extract_mysql_view_metadata(&catalog, &show).unwrap();
    assert_eq!(metadata.check_option, "CASCADED");
}

#[test]
fn mysql_view_metadata_preserves_plain_local_check_option_value() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "LOCAL",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 WITH LOCAL CHECK OPTION",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    let metadata = extract_mysql_view_metadata(&catalog, &show).unwrap();
    assert_eq!(metadata.check_option, "LOCAL");
}

#[test]
fn mysql_view_metadata_rejects_misplaced_check_option_phrase() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "CASCADED",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    for ddl in [
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 WITH CASCADED CHECK OPTION UNION SELECT 2",
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 WITH LOCAL CHECK OPTION + 2",
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 /*!50013 EXTRA WITH CASCADED CHECK OPTION */",
    ] {
        let show = mysql_show_view_result(ddl, "utf8mb4", "utf8mb4_0900_ai_ci");
        assert!(
            extract_mysql_view_metadata(&catalog, &show).is_err(),
            "CHECK OPTION must be a recognized suffix: {ddl}"
        );
    }
}
