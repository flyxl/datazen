//! Startup argv contract used by `datazen open`.
//!
//! The CLI launches the desktop executable directly (or through macOS `open`)
//! with these flags; parsing lives here so it is a pure, testable boundary.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenConnectionRequest {
    pub connection_id: String,
    pub query: Option<String>,
    /// App-data directory forwarded by `datazen open --data-dir`.
    pub data_dir: Option<String>,
    /// Handshake file the desktop writes once the open event has been emitted.
    pub ack_path: Option<String>,
}

const CONNECTION: &str = "--datazen-open-connection";
const QUERY: &str = "--datazen-open-query";
const DATA_DIR: &str = "--datazen-data-dir";
const ACK: &str = "--datazen-open-ack";

pub fn parse_open_connection_args(
    args: &[String],
) -> Result<Option<OpenConnectionRequest>, String> {
    let mut connection_id = None;
    let mut query = None;
    let mut data_dir = None;
    let mut ack_path = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            CONNECTION => {
                if connection_id.is_some() {
                    return Err("--datazen-open-connection was provided more than once".into());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or("--datazen-open-connection requires a connection id")?;
                connection_id = Some(value.clone());
                index += 2;
            }
            QUERY => {
                if query.is_some() {
                    return Err("--datazen-open-query was provided more than once".into());
                }
                let value = args
                    .get(index + 1)
                    .ok_or("--datazen-open-query requires a SQL statement")?;
                query = Some(value.clone());
                index += 2;
            }
            DATA_DIR => {
                if data_dir.is_some() {
                    return Err("--datazen-data-dir was provided more than once".into());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty())
                    .ok_or("--datazen-data-dir requires a path")?;
                data_dir = Some(value.clone());
                index += 2;
            }
            ACK => {
                if ack_path.is_some() {
                    return Err("--datazen-open-ack was provided more than once".into());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty())
                    .ok_or("--datazen-open-ack requires a path")?;
                ack_path = Some(value.clone());
                index += 2;
            }
            _ => index += 1,
        }
    }
    match (connection_id, query, data_dir, ack_path) {
        (None, None, None, None) => Ok(None),
        (None, _, _, _) => Err("--datazen-open-* options require --datazen-open-connection".into()),
        (Some(connection_id), query, data_dir, ack_path) => Ok(Some(OpenConnectionRequest {
            connection_id,
            query,
            data_dir,
            ack_path,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn no_open_flags_is_none() {
        assert_eq!(
            parse_open_connection_args(&args(&["DataZen", "--other"])),
            Ok(None)
        );
    }

    #[test]
    fn parses_connection_and_optional_query() {
        assert_eq!(
            parse_open_connection_args(&args(&["DataZen", "--datazen-open-connection", "conn-1",])),
            Ok(Some(OpenConnectionRequest {
                connection_id: "conn-1".into(),
                query: None,
                data_dir: None,
                ack_path: None,
            }))
        );
        assert_eq!(
            parse_open_connection_args(&args(&[
                "DataZen",
                "--datazen-open-query",
                "SELECT 1",
                "--datazen-open-connection",
                "conn-1",
            ])),
            Ok(Some(OpenConnectionRequest {
                connection_id: "conn-1".into(),
                query: Some("SELECT 1".into()),
                data_dir: None,
                ack_path: None,
            }))
        );
    }

    #[test]
    fn parses_forwarded_data_dir() {
        assert_eq!(
            parse_open_connection_args(&args(&[
                "DataZen",
                "--datazen-open-connection",
                "conn-1",
                "--datazen-data-dir",
                "/tmp/datazen-test",
            ])),
            Ok(Some(OpenConnectionRequest {
                connection_id: "conn-1".into(),
                query: None,
                data_dir: Some("/tmp/datazen-test".into()),
                ack_path: None,
            }))
        );
    }

    #[test]
    fn parses_forwarded_ack_path() {
        assert_eq!(
            parse_open_connection_args(&args(&[
                "DataZen",
                "--datazen-open-connection",
                "conn-1",
                "--datazen-open-ack",
                "/tmp/datazen-ack",
            ])),
            Ok(Some(OpenConnectionRequest {
                connection_id: "conn-1".into(),
                query: None,
                data_dir: None,
                ack_path: Some("/tmp/datazen-ack".into()),
            }))
        );
    }

    #[test]
    fn rejects_missing_values_and_query_without_connection() {
        assert!(
            parse_open_connection_args(&args(&["DataZen", "--datazen-open-connection"])).is_err()
        );
        assert!(parse_open_connection_args(&args(&[
            "DataZen",
            "--datazen-open-connection",
            "--datazen-open-query",
            "SELECT 1",
        ]))
        .is_err());
        assert!(parse_open_connection_args(&args(&["DataZen", "--datazen-open-query"])).is_err());
    }
}
