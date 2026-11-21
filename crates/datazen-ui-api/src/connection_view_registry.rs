pub const BUILTIN_DOCUMENT_MODE: &str = "document";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionViewSelection {
    Document,
    DriverProvided { mode: String },
}

pub fn is_builtin_document_mode(mode: &str) -> bool {
    mode == BUILTIN_DOCUMENT_MODE
}

pub fn get_connection_view(mode: &str, driver_modes: &[String]) -> ConnectionViewSelection {
    if driver_modes.iter().any(|item| item == mode) {
        return ConnectionViewSelection::DriverProvided {
            mode: mode.to_owned(),
        };
    }
    ConnectionViewSelection::Document
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn builtin_document_mode_is_a_single_literal() {
        assert_eq!(BUILTIN_DOCUMENT_MODE, "document");
        assert!(is_builtin_document_mode("document"));
        assert!(!is_builtin_document_mode("Document"));
        assert!(!is_builtin_document_mode("keyvalue"));
        assert!(!is_builtin_document_mode(""));
    }

    #[test]
    fn driver_view_wins_over_every_builtin_mode() {
        let driver = modes(&["document", "keyvalue"]);
        assert_eq!(
            get_connection_view("document", &driver),
            ConnectionViewSelection::DriverProvided {
                mode: "document".to_owned()
            }
        );
        assert_eq!(
            get_connection_view("keyvalue", &driver),
            ConnectionViewSelection::DriverProvided {
                mode: "keyvalue".to_owned()
            }
        );
    }

    #[test]
    fn builtin_and_unknown_modes_fall_back_to_document() {
        assert_eq!(
            get_connection_view("document", &[]),
            ConnectionViewSelection::Document
        );
        assert_eq!(
            get_connection_view("unknown", &[]),
            ConnectionViewSelection::Document
        );
        assert_eq!(
            get_connection_view("", &[]),
            ConnectionViewSelection::Document
        );
        let driver = modes(&["redis", "mongodb"]);
        assert_eq!(
            get_connection_view("document", &driver),
            ConnectionViewSelection::Document
        );
        assert_eq!(
            get_connection_view("unknown", &driver),
            ConnectionViewSelection::Document
        );
    }
}
