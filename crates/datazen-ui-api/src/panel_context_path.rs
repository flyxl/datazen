#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelContextPathArgs<'a> {
    pub connection_name: &'a str,
    pub database: Option<&'a str>,
    pub schema: Option<&'a str>,
}

pub fn format_panel_context_path(args: &PanelContextPathArgs<'_>) -> String {
    let mut parts = vec![args.connection_name];
    if let Some(database) = args
        .database
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(database);
    }
    if let Some(schema) = args.schema.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(schema);
    }
    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_parts_are_joined_in_order() {
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "conn",
                database: Some("shop"),
                schema: Some("public"),
            }),
            "conn.shop.public"
        );
    }

    #[test]
    fn missing_or_blank_optional_parts_are_omitted() {
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "conn",
                database: None,
                schema: None,
            }),
            "conn"
        );
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "conn",
                database: Some("  "),
                schema: Some("\t\n"),
            }),
            "conn"
        );
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "conn",
                database: Some("shop"),
                schema: None,
            }),
            "conn.shop"
        );
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "conn",
                database: None,
                schema: Some("public"),
            }),
            "conn.public"
        );
    }

    #[test]
    fn optional_parts_are_trimmed_but_connection_name_is_not() {
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "  conn  ",
                database: Some("  shop  "),
                schema: Some("  public  "),
            }),
            "  conn  .shop.public"
        );
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "",
                database: Some("shop"),
                schema: None,
            }),
            ".shop"
        );
        assert_eq!(
            format_panel_context_path(&PanelContextPathArgs {
                connection_name: "",
                database: None,
                schema: None,
            }),
            ""
        );
    }
}
