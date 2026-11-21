#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    String(String),
    Error {
        message: String,
        cause: Option<Box<HostError>>,
    },
    Object {
        message: Option<String>,
    },
    Other(String),
}

impl HostError {
    pub fn representation(&self) -> String {
        match self {
            HostError::String(text) => text.clone(),
            HostError::Error { message, .. } => format!("Error: {message}"),
            HostError::Object { .. } => "[object Object]".to_owned(),
            HostError::Other(text) => text.clone(),
        }
    }
}

pub fn extract_host_error_message(error: &HostError) -> String {
    let mut messages: Vec<String> = Vec::new();
    let mut current: Option<&HostError> = Some(error);
    let mut depth = 0;
    while depth < 5 {
        let Some(value) = current else {
            break;
        };
        match value {
            HostError::String(text) => {
                messages.push(text.clone());
                break;
            }
            HostError::Error { message, cause } => {
                messages.push(message.clone());
                current = cause.as_deref();
                depth += 1;
            }
            HostError::Object { message } => {
                if let Some(message) = message {
                    messages.push(message.clone());
                }
                break;
            }
            HostError::Other(_) => break,
        }
    }
    match messages.last() {
        Some(message) if !message.trim().is_empty() => message.clone(),
        _ => error.representation(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_string_is_returned_verbatim() {
        assert_eq!(
            extract_host_error_message(&HostError::String("boom".to_owned())),
            "boom"
        );
    }

    #[test]
    fn the_deepest_cause_message_wins() {
        let error = HostError::Error {
            message: "rpc failed: inner".to_owned(),
            cause: Some(Box::new(HostError::Error {
                message: "grid.cell.readOnly: nope".to_owned(),
                cause: Some(Box::new(HostError::String("root".to_owned()))),
            })),
        };
        assert_eq!(extract_host_error_message(&error), "root");
    }

    #[test]
    fn intermediate_error_message_is_used_when_the_cause_is_not_a_message() {
        let error = HostError::Error {
            message: "rpc failed: grid.cell.readOnly: nope".to_owned(),
            cause: Some(Box::new(HostError::Other("ignored".to_owned()))),
        };
        assert_eq!(
            extract_host_error_message(&error),
            "rpc failed: grid.cell.readOnly: nope"
        );
    }

    #[test]
    fn object_message_is_pushed_and_stops_the_walk() {
        let error = HostError::Error {
            message: "outer".to_owned(),
            cause: Some(Box::new(HostError::Object {
                message: Some("inner".to_owned()),
            })),
        };
        assert_eq!(extract_host_error_message(&error), "inner");
        let no_message = HostError::Error {
            message: "outer".to_owned(),
            cause: Some(Box::new(HostError::Object { message: None })),
        };
        assert_eq!(extract_host_error_message(&no_message), "outer");
    }

    #[test]
    fn blank_original_falls_back_to_the_error_representation() {
        assert_eq!(
            extract_host_error_message(&HostError::String("   ".to_owned())),
            "   "
        );
        let error = HostError::Error {
            message: "  ".to_owned(),
            cause: None,
        };
        assert_eq!(extract_host_error_message(&error), "Error:   ");
        assert_eq!(
            extract_host_error_message(&HostError::Object { message: Some("   ".to_owned()) }),
            "[object Object]"
        );
        assert_eq!(
            extract_host_error_message(&HostError::Other("weird".to_owned())),
            "weird"
        );
    }

    #[test]
    fn cause_walk_is_capped_at_five_levels() {
        let mut error = HostError::String("level5".to_owned());
        for level in (1..5).rev() {
            error = HostError::Error {
                message: format!("level{level}"),
                cause: Some(Box::new(error)),
            };
        }
        assert_eq!(extract_host_error_message(&error), "level5");
        let mut deeper = HostError::String("level6".to_owned());
        for level in (1..6).rev() {
            deeper = HostError::Error {
                message: format!("level{level}"),
                cause: Some(Box::new(deeper)),
            };
        }
        assert_eq!(extract_host_error_message(&deeper), "level5");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::extract_host_error_message;
        let _: Option<crate::HostError> = None;
    }
}
