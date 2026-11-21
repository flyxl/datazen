use std::fmt;

pub enum ErrorLike<'a> {
    Error { message: &'a str },
    Value(&'a dyn fmt::Display),
}

pub fn to_error_message(err: &ErrorLike<'_>) -> String {
    match err {
        ErrorLike::Error { message } => (*message).to_owned(),
        ErrorLike::Value(value) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_values_use_their_message_verbatim() {
        assert_eq!(
            to_error_message(&ErrorLike::Error {
                message: "connection failed"
            }),
            "connection failed"
        );
        assert_eq!(to_error_message(&ErrorLike::Error { message: "" }), "");
        assert_eq!(
            to_error_message(&ErrorLike::Error {
                message: "  spaced  "
            }),
            "  spaced  "
        );
        assert_eq!(
            to_error_message(&ErrorLike::Error {
                message: "line one\nline two"
            }),
            "line one\nline two"
        );
    }

    #[test]
    fn non_error_values_use_their_display_form() {
        let number = 42;
        let flag = false;
        let text = "plain";
        let symbol = 'x';
        assert_eq!(to_error_message(&ErrorLike::Value(&number)), "42");
        assert_eq!(to_error_message(&ErrorLike::Value(&flag)), "false");
        assert_eq!(to_error_message(&ErrorLike::Value(&text)), "plain");
        assert_eq!(to_error_message(&ErrorLike::Value(&symbol)), "x");
    }
}
