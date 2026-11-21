use crate::base64_to_bytes;

use super::types::{is_supported_encoding, ImportParseError, ImportParseErrorKind};

pub trait ImportTextDecoder {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Result<String, ImportParseError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Utf8TextDecoder;

impl ImportTextDecoder for Utf8TextDecoder {
    fn decode(&self, bytes: &[u8], encoding: &str) -> Result<String, ImportParseError> {
        if !is_supported_encoding(encoding) {
            return Err(ImportParseError::new(
                ImportParseErrorKind::UnsupportedEncoding,
                format!("The encoding \"{encoding}\" is not supported"),
            ));
        }
        if encoding.eq_ignore_ascii_case("utf-8") || encoding.eq_ignore_ascii_case("utf8") {
            return Ok(String::from_utf8_lossy(bytes).into_owned());
        }
        Err(ImportParseError::new(
            ImportParseErrorKind::UnsupportedEncoding,
            format!("The encoding \"{encoding}\" is not available in this build"),
        ))
    }
}

pub fn decode_import_text(
    bytes: &[u8],
    encoding: &str,
    decoder: &dyn ImportTextDecoder,
) -> Result<String, ImportParseError> {
    decoder.decode(bytes, encoding)
}

pub fn decode_import_base64(
    data_base64: &str,
    encoding: &str,
    decoder: &dyn ImportTextDecoder,
) -> Result<String, ImportParseError> {
    let bytes = base64_to_bytes(data_base64).map_err(|error| {
        ImportParseError::new(ImportParseErrorKind::InvalidBase64, error.to_string())
    })?;
    decode_import_text(&bytes, encoding, decoder)
}
