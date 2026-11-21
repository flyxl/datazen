use crate::Translate;

const FORMAT_LABELS: [&str; 3] = ["DataZen", "TablePlus", "Encrypted connection"];

const KEY_PASSWORD_REQUIRED: &str = "connShare.error.passwordRequired";
const KEY_ENCRYPTED_IMPORT_PASSWORD_REQUIRED: &str =
    "connShare.error.encryptedImportPasswordRequired";
const KEY_DBX_ENCRYPTED_PASSWORD_REQUIRED: &str = "connShare.error.dbxEncryptedPasswordRequired";
const KEY_DATAZEN_ENCRYPTED_PASSWORD_REQUIRED: &str =
    "connShare.error.datazenEncryptedPasswordRequired";
const KEY_DATAZEN_PASSWORD_REQUIRED: &str = "connShare.error.datazenPasswordRequired";
const KEY_INVALID_UTF8: &str = "connShare.error.invalidUtf8";
const KEY_UNRECOGNIZED_FORMAT: &str = "connShare.error.unrecognizedFormat";
const KEY_NO_FILES: &str = "connShare.error.noFiles";
const KEY_DECRYPTION_WRONG_PASSWORD: &str = "connShare.error.decryptionWrongPassword";
const KEY_MISSING_CONNECTIONS_FIELD: &str = "connShare.error.missingConnectionsField";
const KEY_MISSING_CONNECTIONS: &str = "connShare.error.missingConnections";
const KEY_DBX_WRONG_PASSPHRASE: &str = "connShare.error.dbxWrongPassphrase";
const KEY_DBX_MISSING_CONNECTIONS: &str = "connShare.error.dbxMissingConnections";
const KEY_FORMAT_PASSWORD_REQUIRED: &str = "connShare.error.formatPasswordRequired";
const KEY_INVALID_FILE_TOO_SHORT: &str = "connShare.error.invalidFileTooShort";
const KEY_INVALID_FILE_ENCRYPTION_TYPE: &str = "connShare.error.invalidFileEncryptionType";
const KEY_INVALID_FILE_RNCRYPTOR_VERSION: &str = "connShare.error.invalidFileRncryptorVersion";
const KEY_DECRYPTION_FAILED_LEGACY_HINT: &str = "connShare.error.decryptionFailedLegacyHint";
const KEY_DECRYPTION_FAILED: &str = "connShare.error.decryptionFailed";
const KEY_INVALID_JSON_AFTER_DECRYPT: &str = "connShare.error.invalidJsonAfterDecrypt";
const KEY_PAYLOAD_NOT_UTF8: &str = "connShare.error.payloadNotUtf8";
const KEY_READ_FAILED: &str = "connShare.error.readFailed";
const KEY_UNKNOWN_IMPORT_SOURCE: &str = "connShare.error.unknownImportSource";
const KEY_IMPORT_SUCCESS_WITH_SKIPPED: &str = "connShare.importSuccessWithSkipped";
const KEY_IMPORT_SUCCESS: &str = "connShare.importSuccess";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionImportResult {
    pub imported: u64,
    pub overwritten: u64,
    pub groups_added: u64,
    pub skipped: Option<Vec<String>>,
}

pub enum ShareErrorSource<'a> {
    String(&'a str),
    Error { message: &'a str },
    Other,
}

fn extract_format_label(message: &str) -> Option<&'static str> {
    FORMAT_LABELS
        .iter()
        .copied()
        .find(|label| message.contains(label))
}

fn exact_key(message: &str) -> Option<&'static str> {
    match message {
        "Password is required" => Some(KEY_PASSWORD_REQUIRED),
        "Password is required for encrypted connection import" => {
            Some(KEY_ENCRYPTED_IMPORT_PASSWORD_REQUIRED)
        }
        "Password is required for DBX encrypted import" => {
            Some(KEY_DBX_ENCRYPTED_PASSWORD_REQUIRED)
        }
        "Password is required for DataZen encrypted import" => {
            Some(KEY_DATAZEN_ENCRYPTED_PASSWORD_REQUIRED)
        }
        "Password is required for DataZen import" => Some(KEY_DATAZEN_PASSWORD_REQUIRED),
        "Import file is not valid UTF-8 text (for DataZen use .datazenconnection)" => {
            Some(KEY_INVALID_UTF8)
        }
        "Unrecognized connection import format" => Some(KEY_UNRECOGNIZED_FORMAT),
        "No connection files to import" => Some(KEY_NO_FILES),
        "Decryption failed: wrong password" => Some(KEY_DECRYPTION_WRONG_PASSWORD),
        "Invalid import file: missing 'connections' field" => Some(KEY_MISSING_CONNECTIONS_FIELD),
        "missing connections" => Some(KEY_MISSING_CONNECTIONS),
        "DBX decryption failed: wrong passphrase" => Some(KEY_DBX_WRONG_PASSPHRASE),
        "DBX import: missing connections array" => Some(KEY_DBX_MISSING_CONNECTIONS),
        _ => None,
    }
}

pub fn translate_connection_share_error(
    message: &str,
    t: &dyn Translate,
    fallback: &str,
) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return fallback.to_owned();
    }
    if let Some(key) = exact_key(trimmed) {
        return t.translate(key, &[]);
    }
    if let Some(format) = extract_format_label(trimmed) {
        if trimmed == format!("Password is required for {format} import") {
            return t.translate(
                KEY_FORMAT_PASSWORD_REQUIRED,
                &[("format", format.to_owned())],
            );
        }
        if trimmed == format!("Invalid {format} file: too short") {
            return t.translate(KEY_INVALID_FILE_TOO_SHORT, &[("format", format.to_owned())]);
        }
        if trimmed == format!("Invalid {format} file: expected password-based encryption") {
            return t.translate(
                KEY_INVALID_FILE_ENCRYPTION_TYPE,
                &[("format", format.to_owned())],
            );
        }
        if trimmed.starts_with(&format!(
            "Invalid {format} file: unsupported RNCryptor version"
        )) {
            return t.translate(
                KEY_INVALID_FILE_RNCRYPTOR_VERSION,
                &[("format", format.to_owned())],
            );
        }
        if trimmed.starts_with(&format!(
            "{format} decryption failed: wrong password or corrupt file"
        )) && trimmed.contains("Legacy DataZen")
        {
            return t.translate(
                KEY_DECRYPTION_FAILED_LEGACY_HINT,
                &[("format", format.to_owned())],
            );
        }
        if trimmed == format!("{format} decryption failed: wrong password or corrupt file") {
            return t.translate(KEY_DECRYPTION_FAILED, &[("format", format.to_owned())]);
        }
        if trimmed.starts_with(&format!("Invalid {format} JSON after decrypt:")) {
            return t.translate(
                KEY_INVALID_JSON_AFTER_DECRYPT,
                &[("format", format.to_owned())],
            );
        }
        if trimmed.starts_with(&format!("{format} payload is not UTF-8:")) {
            return t.translate(KEY_PAYLOAD_NOT_UTF8, &[("format", format.to_owned())]);
        }
    }
    if trimmed.starts_with("Failed to read ") {
        return t.translate(KEY_READ_FAILED, &[]);
    }
    if trimmed.starts_with("Unknown import source: ") {
        return t.translate(KEY_UNKNOWN_IMPORT_SOURCE, &[]);
    }
    trimmed.to_owned()
}

pub fn ipc_connection_share_error(
    source: ShareErrorSource<'_>,
    t: &dyn Translate,
    fallback: &str,
) -> String {
    let raw = match source {
        ShareErrorSource::String(message) if !message.trim().is_empty() => message,
        ShareErrorSource::Error { message } if !message.trim().is_empty() => message,
        _ => return fallback.to_owned(),
    };
    translate_connection_share_error(raw, t, fallback)
}

pub fn format_connection_import_success(
    result: &ConnectionImportResult,
    t: &dyn Translate,
) -> String {
    let skipped = result.skipped.as_ref().map_or(0, Vec::len);
    if skipped > 0 {
        let skipped_text = result
            .skipped
            .as_ref()
            .map(|items| items.join(","))
            .unwrap_or_default();
        return t.translate(
            KEY_IMPORT_SUCCESS_WITH_SKIPPED,
            &[
                ("imported", result.imported.to_string()),
                ("overwritten", result.overwritten.to_string()),
                ("groupsAdded", result.groups_added.to_string()),
                ("skipped", skipped_text),
            ],
        );
    }
    t.translate(
        KEY_IMPORT_SUCCESS,
        &[
            ("imported", result.imported.to_string()),
            ("overwritten", result.overwritten.to_string()),
            ("groupsAdded", result.groups_added.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn echo(key: &str, params: &[(&str, String)]) -> String {
        if params.is_empty() {
            return key.to_owned();
        }
        let mut text = key.to_owned();
        for (name, value) in params {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    struct Recorder<'a> {
        calls: &'a RefCell<Vec<(String, Vec<(String, String)>)>>,
    }

    impl Translate for Recorder<'_> {
        fn translate(&self, key: &str, params: &[(&str, String)]) -> String {
            self.calls.borrow_mut().push((
                key.to_owned(),
                params
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect(),
            ));
            echo(key, params)
        }
    }

    fn result(skipped: Option<Vec<&str>>) -> ConnectionImportResult {
        ConnectionImportResult {
            imported: 2,
            overwritten: 1,
            groups_added: 3,
            skipped: skipped.map(|items| items.into_iter().map(str::to_owned).collect()),
        }
    }

    #[test]
    fn exact_messages_map_to_their_keys() {
        let cases = [
            ("Password is required", KEY_PASSWORD_REQUIRED),
            (
                "Password is required for encrypted connection import",
                KEY_ENCRYPTED_IMPORT_PASSWORD_REQUIRED,
            ),
            (
                "Password is required for DBX encrypted import",
                KEY_DBX_ENCRYPTED_PASSWORD_REQUIRED,
            ),
            (
                "Password is required for DataZen encrypted import",
                KEY_DATAZEN_ENCRYPTED_PASSWORD_REQUIRED,
            ),
            (
                "Password is required for DataZen import",
                KEY_DATAZEN_PASSWORD_REQUIRED,
            ),
            (
                "Import file is not valid UTF-8 text (for DataZen use .datazenconnection)",
                KEY_INVALID_UTF8,
            ),
            (
                "Unrecognized connection import format",
                KEY_UNRECOGNIZED_FORMAT,
            ),
            ("No connection files to import", KEY_NO_FILES),
            (
                "Decryption failed: wrong password",
                KEY_DECRYPTION_WRONG_PASSWORD,
            ),
            (
                "Invalid import file: missing 'connections' field",
                KEY_MISSING_CONNECTIONS_FIELD,
            ),
            ("missing connections", KEY_MISSING_CONNECTIONS),
            (
                "DBX decryption failed: wrong passphrase",
                KEY_DBX_WRONG_PASSPHRASE,
            ),
            (
                "DBX import: missing connections array",
                KEY_DBX_MISSING_CONNECTIONS,
            ),
        ];
        for (message, key) in cases {
            assert_eq!(
                translate_connection_share_error(message, &echo, "fallback"),
                key
            );
            let padded = format!("  {message}  ");
            assert_eq!(
                translate_connection_share_error(&padded, &echo, "fallback"),
                key
            );
        }
    }

    #[test]
    fn format_specific_messages_pass_only_the_format_param() {
        let cases = [
            (
                "Password is required for DataZen import",
                Some(KEY_DATAZEN_PASSWORD_REQUIRED),
            ),
            (
                "Password is required for TablePlus import",
                Some(KEY_FORMAT_PASSWORD_REQUIRED),
            ),
            (
                "Invalid TablePlus file: too short",
                Some(KEY_INVALID_FILE_TOO_SHORT),
            ),
            (
                "Invalid DataZen file: expected password-based encryption",
                Some(KEY_INVALID_FILE_ENCRYPTION_TYPE),
            ),
            (
                "Invalid DataZen file: unsupported RNCryptor version 3",
                Some(KEY_INVALID_FILE_RNCRYPTOR_VERSION),
            ),
            (
                "DataZen decryption failed: wrong password or corrupt file",
                Some(KEY_DECRYPTION_FAILED),
            ),
            (
                "DataZen decryption failed: wrong password or corrupt file (Legacy DataZen)",
                Some(KEY_DECRYPTION_FAILED_LEGACY_HINT),
            ),
            (
                "Invalid DataZen JSON after decrypt: { bad",
                Some(KEY_INVALID_JSON_AFTER_DECRYPT),
            ),
            (
                "DataZen payload is not UTF-8: ff fe",
                Some(KEY_PAYLOAD_NOT_UTF8),
            ),
        ];
        let calls = RefCell::new(Vec::new());
        let recorder = Recorder { calls: &calls };
        for (message, key) in cases {
            calls.borrow_mut().clear();
            let rendered =
                translate_connection_share_error(message, &recorder as &dyn Translate, "fallback");
            assert_eq!(rendered, key.unwrap_or("fallback"));
            let logged = calls.borrow();
            assert_eq!(logged.len(), 1);
            if logged[0].0.contains("format") {
                assert_eq!(logged[0].1.len(), 1);
                assert!(["DataZen", "TablePlus", "Encrypted connection"]
                    .contains(&logged[0].1[0].1.as_str()));
            }
        }
    }

    #[test]
    fn exact_match_wins_over_a_format_label() {
        let calls = RefCell::new(Vec::new());
        let recorder = Recorder { calls: &calls };
        translate_connection_share_error(
            "Password is required for DataZen import",
            &recorder as &dyn Translate,
            "fallback",
        );
        assert_eq!(calls.borrow()[0].0, KEY_DATAZEN_PASSWORD_REQUIRED);
        assert!(calls.borrow()[0].1.is_empty());
    }

    #[test]
    fn decrypt_and_payload_errors_do_not_echo_the_tail() {
        let calls = RefCell::new(Vec::new());
        let recorder = Recorder { calls: &calls };
        for message in [
            "Invalid DataZen JSON after decrypt: {\"password\":\"super-secret\"}",
            "DataZen payload is not UTF-8: c3 28 super-secret",
            "DataZen decryption failed: wrong password or corrupt file (Legacy DataZen super-secret)",
            "Invalid DataZen file: unsupported RNCryptor version super-secret",
        ] {
            let rendered = translate_connection_share_error(message, &recorder as &dyn Translate, "fallback");
            assert!(!rendered.contains("super-secret"));
        }
        for (_, params) in calls.borrow().iter() {
            for (_, value) in params {
                assert!(!value.contains("super-secret"));
            }
        }
    }

    #[test]
    fn prefix_and_passthrough_rules() {
        assert_eq!(
            translate_connection_share_error("Failed to read /tmp/x", &echo, "fallback"),
            KEY_READ_FAILED
        );
        assert_eq!(
            translate_connection_share_error("Unknown import source: foo", &echo, "fallback"),
            KEY_UNKNOWN_IMPORT_SOURCE
        );
        assert_eq!(
            translate_connection_share_error("  Some new backend error  ", &echo, "fallback"),
            "Some new backend error"
        );
        assert_eq!(
            translate_connection_share_error("   ", &echo, "fallback"),
            "fallback"
        );
    }

    #[test]
    fn ipc_sources_are_normalized_before_translation() {
        assert_eq!(
            ipc_connection_share_error(ShareErrorSource::String("  "), &echo, "fallback"),
            "fallback"
        );
        assert_eq!(
            ipc_connection_share_error(
                ShareErrorSource::Error { message: "   " },
                &echo,
                "fallback"
            ),
            "fallback"
        );
        assert_eq!(
            ipc_connection_share_error(ShareErrorSource::Other, &echo, "fallback"),
            "fallback"
        );
        assert_eq!(
            ipc_connection_share_error(
                ShareErrorSource::String("No connection files to import"),
                &echo,
                "fallback"
            ),
            KEY_NO_FILES
        );
        assert_eq!(
            ipc_connection_share_error(
                ShareErrorSource::Error {
                    message: "Unrecognized connection import format"
                },
                &echo,
                "fallback"
            ),
            KEY_UNRECOGNIZED_FORMAT
        );
    }

    #[test]
    fn import_success_uses_the_skipped_or_plain_key_with_all_counters() {
        let calls = RefCell::new(Vec::new());
        let recorder = Recorder { calls: &calls };
        format_connection_import_success(&result(None), &recorder as &dyn Translate);
        assert_eq!(calls.borrow()[0].0, KEY_IMPORT_SUCCESS);
        assert_eq!(calls.borrow()[0].1.len(), 3);
        format_connection_import_success(&result(Some(Vec::new())), &recorder as &dyn Translate);
        assert_eq!(calls.borrow()[1].0, KEY_IMPORT_SUCCESS);
        format_connection_import_success(
            &result(Some(vec!["oracle", "mongo"])),
            &recorder as &dyn Translate,
        );
        assert_eq!(calls.borrow()[2].0, KEY_IMPORT_SUCCESS_WITH_SKIPPED);
        assert_eq!(
            calls.borrow()[2].1[3],
            ("skipped".to_owned(), "oracle,mongo".to_owned())
        );
    }
}
