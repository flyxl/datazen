    #[test]
    fn sql_main_keyword_simple_select() {
        assert_eq!(sql_main_keyword("SELECT 1"), Some("SELECT".to_string()));
    }

    #[test]
    fn sql_main_keyword_with_cte_select() {
        assert_eq!(
            sql_main_keyword("WITH cte AS (SELECT 1) SELECT * FROM cte"),
            Some("SELECT".to_string())
        );
    }


    #[test]
    fn sql_main_keyword_with_cte_delete() {
        assert_eq!(
            sql_main_keyword(
                "WITH t AS (SELECT id FROM u) DELETE FROM u WHERE id IN (SELECT id FROM t)"
            ),
            Some("DELETE".to_string())
        );
    }


    #[test]
    fn sql_main_keyword_strips_comments() {
        assert_eq!(
            sql_main_keyword("-- drop hint\nSELECT 1 /* alter */"),
            Some("SELECT".to_string())
        );
    }


    #[test]
    fn sql_main_keyword_empty_returns_none() {
        assert_eq!(sql_main_keyword("   "), None);
        assert_eq!(sql_main_keyword("-- only comment"), None);
    }


    #[test]
    fn sql_main_keyword_respects_quoted_identifiers() {
        assert_eq!(
            sql_main_keyword("SELECT `DROP` FROM t"),
            Some("SELECT".to_string())
        );
    }


    #[test]
    fn sql_main_keyword_block_comment_stripped() {
        assert_eq!(
            sql_main_keyword("/* DROP */ SELECT 1"),
            Some("SELECT".to_string())
        );
    }


    #[test]
    fn sql_main_keyword_handles_fullwidth() {
        assert_eq!(
            sql_main_keyword("ＤＲＯＰ TABLE t"),
            Some("DROP".to_string())
        );
        assert_eq!(
            sql_main_keyword("ＳＥＬＥＣＴ 1"),
            Some("SELECT".to_string())
        );
    }
