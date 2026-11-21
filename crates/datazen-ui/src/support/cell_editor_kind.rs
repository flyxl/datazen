//! Cell editor kind resolution, ported from the kind half of
//! `packages/frontend/src/lib/cellEditorKind.ts`.
//!
//! This is a projection of [`CellType`]: the grid's editor control must agree
//! with the colour it paints, so the decision is made once here rather than
//! inline in the grid.
//!
//! The error half of the same TS file (`CellValueErrorReason`, the sentence
//! vocabulary `cellValueErrorMessageKey` keys on) is mirrored below — it
//! landed with the active-editor registry (T405), whose `commit()` returns
//! one.
#![allow(dead_code)] // consumers: T015 GridCell (which editor to open)

use crate::support::cell_types::CellType;

/// The editor control a cell needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellEditorKind {
    Text,
    Number,
    Boolean,
    Enum,
    Date,
    Json,
    Binary,
    LongText,
}

impl CellEditorKind {
    /// Stable identifier, handy for assertions and diagnostics.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Enum => "enum",
            Self::Date => "date",
            Self::Json => "json",
            Self::Binary => "binary",
            Self::LongText => "longText",
        }
    }
}

/// A value longer than this is edited as a long text, not inline.
pub(crate) const INLINE_TEXT_MAX_CODE_POINTS: usize = 200;

/// The inline editor never holds more than 64 KiB.
pub(crate) const CELL_EDIT_MAX_BYTES: usize = 64 * 1024;

/// Everything the kind resolution needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellEditorKindInput {
    pub cell_type: CellType,
    /// The value carries a time zone (a picker cannot hold an offset).
    pub zoned: bool,
    /// The column has a candidate list; currently unused by the dispatch but
    /// part of the React contract.
    pub has_candidates: bool,
    pub value_code_points: usize,
    pub has_line_break: bool,
}

/// Resolve the editor kind.
///
/// Zoned time and datetime cells fall back to text on purpose: silently
/// dropping the offset would move the instant. `uuid` is always text because
/// there is no dedicated editor.
pub(crate) fn resolve_cell_editor_kind(input: CellEditorKindInput) -> CellEditorKind {
    match input.cell_type {
        CellType::Bool => CellEditorKind::Boolean,
        CellType::Enum => CellEditorKind::Enum,
        CellType::Date => CellEditorKind::Date,
        CellType::Time => {
            if input.zoned {
                CellEditorKind::Text
            } else {
                CellEditorKind::Date
            }
        }
        CellType::DateTime => {
            if input.zoned {
                CellEditorKind::Text
            } else {
                CellEditorKind::Date
            }
        }
        CellType::Number => CellEditorKind::Number,
        CellType::Json => CellEditorKind::Json,
        CellType::Bytes => CellEditorKind::Binary,
        CellType::Text | CellType::Unknown => {
            if input.value_code_points > INLINE_TEXT_MAX_CODE_POINTS || input.has_line_break {
                CellEditorKind::LongText
            } else {
                CellEditorKind::Text
            }
        }
        CellType::Uuid => CellEditorKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(cell_type: CellType) -> CellEditorKindInput {
        CellEditorKindInput {
            cell_type,
            zoned: false,
            has_candidates: false,
            value_code_points: 0,
            has_line_break: false,
        }
    }

    #[test]
    fn maps_every_plain_cell_type() {
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Bool)),
            CellEditorKind::Boolean
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Enum)),
            CellEditorKind::Enum
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Date)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Time)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::DateTime)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Number)),
            CellEditorKind::Number
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Json)),
            CellEditorKind::Json
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Bytes)),
            CellEditorKind::Binary
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Text)),
            CellEditorKind::Text
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Unknown)),
            CellEditorKind::Text
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Uuid)),
            CellEditorKind::Text
        );
    }

    #[test]
    fn zoned_time_and_datetime_fall_back_to_text() {
        let mut zoned = input(CellType::Time);
        zoned.zoned = true;
        assert_eq!(resolve_cell_editor_kind(zoned), CellEditorKind::Text);

        let mut zoned = input(CellType::DateTime);
        zoned.zoned = true;
        assert_eq!(resolve_cell_editor_kind(zoned), CellEditorKind::Text);
    }

    #[test]
    fn long_or_multiline_text_uses_the_long_editor() {
        let mut long = input(CellType::Text);
        long.value_code_points = INLINE_TEXT_MAX_CODE_POINTS + 1;
        assert_eq!(resolve_cell_editor_kind(long), CellEditorKind::LongText);

        let mut multiline = input(CellType::Unknown);
        multiline.has_line_break = true;
        assert_eq!(
            resolve_cell_editor_kind(multiline),
            CellEditorKind::LongText
        );

        let mut boundary = input(CellType::Text);
        boundary.value_code_points = INLINE_TEXT_MAX_CODE_POINTS;
        assert_eq!(resolve_cell_editor_kind(boundary), CellEditorKind::Text);
    }

    #[test]
    fn uuid_stays_text_even_when_long() {
        let mut uuid = input(CellType::Uuid);
        uuid.value_code_points = INLINE_TEXT_MAX_CODE_POINTS + 10;
        uuid.has_line_break = true;
        assert_eq!(resolve_cell_editor_kind(uuid), CellEditorKind::Text);
    }
}

/// Why an input could not be turned into a write intent — the TS file's
/// `CellValueErrorReason` union.
///
/// Every reason has its own sentence in the interface (05 §8); the editor
/// shows it inline and the paste path reports it per cell. The spelling
/// matches the React union member-for-member, so a wire or test that
/// prints it can compare against the same strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellValueErrorReason {
    EmptyNotNull,
    InvalidNumber,
    InvalidInteger,
    InvalidBoolean,
    InvalidDate,
    InvalidTime,
    InvalidDateTime,
    InvalidJson,
    InvalidUuid,
    TooLarge,
    NotEditable,
}

impl CellValueErrorReason {
    /// The React union's string member (`'emptyNotNull'`, …).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::EmptyNotNull => "emptyNotNull",
            Self::InvalidNumber => "invalidNumber",
            Self::InvalidInteger => "invalidInteger",
            Self::InvalidBoolean => "invalidBoolean",
            Self::InvalidDate => "invalidDate",
            Self::InvalidTime => "invalidTime",
            Self::InvalidDateTime => "invalidDateTime",
            Self::InvalidJson => "invalidJson",
            Self::InvalidUuid => "invalidUuid",
            Self::TooLarge => "tooLarge",
            Self::NotEditable => "notEditable",
        }
    }

    /// `cellValueErrorMessageKey` (`cellEditorKind.ts:589-613`): the
    /// **only** mapping from a reason to a key — every caller reads it
    /// from here, so a reason can never end up without an explanation.
    pub(crate) fn message_key(self) -> &'static str {
        match self {
            Self::EmptyNotNull => "cellEditor.notNull",
            Self::InvalidNumber => "cellEditor.invalidNumber",
            Self::InvalidInteger => "cellEditor.invalidInteger",
            Self::InvalidBoolean => "cellEditor.invalidBoolean",
            Self::InvalidDate => "cellEditor.invalidDate",
            Self::InvalidTime => "cellEditor.invalidTime",
            Self::InvalidDateTime => "cellEditor.invalidDateTime",
            Self::InvalidJson => "cellEditor.invalidJson",
            Self::InvalidUuid => "cellEditor.invalidUuid",
            Self::TooLarge => "cellEditor.valueTooLarge",
            Self::NotEditable => "cellEditor.binaryNotEditable",
        }
    }
}

// ---------------------------------------------------------------------------
// `coerceValue` (`cellEditorKind.ts:209-326`) — the write intent of a typed
// input, ported for the CellEditorHost (T297); the T118 port carried the
// kind resolution and the reasons, and the host needs the gate itself.
// ---------------------------------------------------------------------------

/// React `CoerceInput` (`cellEditorKind.ts:97-110`).
#[derive(Clone, Debug)]
pub(crate) struct CoerceInput<'a> {
    pub kind: CellEditorKind,
    pub cell_type: CellType,
    pub text: &'a str,
    /// The column accepts SQL `NULL`.
    pub nullable: bool,
    /// The value before the edit; the editor needs it for "nothing
    /// changed" (carried for the interface — the text rules never read
    /// it).
    pub original_value: Option<&'a serde_json::Value>,
    /// The column holds whole numbers, so a fraction is refused.
    pub integer_only: bool,
}

/// React `CoerceResult`: the written value, or the refusal's reason.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CoerceResult {
    Ok(serde_json::Value),
    Refused(CellValueErrorReason),
}

/// `coerceValue` (`cellEditorKind.ts:209-237`): the write intent of a
/// typed input — an empty input on a `NOT NULL` column is **refused**
/// (`emptyNotNull`, never an empty string and never a `NULL`), an
/// empty input on a nullable column is `NULL`, a number is validated
/// by a regular expression first and only then converted, `NaN` and
/// `Infinity` are refused, a date without a zone is a wall clock
/// passed through verbatim, and invalid JSON is refused.
pub(crate) fn coerce_value(input: &CoerceInput) -> CoerceResult {
    let text = input.text;
    if text.is_empty() {
        if !input.nullable {
            return CoerceResult::Refused(CellValueErrorReason::EmptyNotNull);
        }
        return CoerceResult::Ok(serde_json::Value::Null);
    }

    match input.kind {
        CellEditorKind::Number => coerce_number(input, text),
        CellEditorKind::Boolean => coerce_boolean(text),
        CellEditorKind::Date => coerce_temporal(input, text),
        CellEditorKind::Json => coerce_json(text),
        CellEditorKind::Enum => CoerceResult::Ok(serde_json::Value::String(text.to_owned())),
        CellEditorKind::Binary => CoerceResult::Refused(CellValueErrorReason::NotEditable),
        CellEditorKind::Text | CellEditorKind::LongText => coerce_text(input, text),
    }
}

/// `coerceNumber` (`cellEditorKind.ts:235-243`): ASCII digits only —
/// `١٢` (Arabic-Indic) and `0x10` are not numbers here.
fn coerce_number(input: &CoerceInput, text: &str) -> CoerceResult {
    if !number_re(text) {
        return CoerceResult::Refused(CellValueErrorReason::InvalidNumber);
    }
    let value: f64 = text.parse().unwrap_or(f64::NAN);
    if !value.is_finite() {
        return CoerceResult::Refused(CellValueErrorReason::InvalidNumber);
    }
    if input.integer_only && value.fract() != 0.0 {
        return CoerceResult::Refused(CellValueErrorReason::InvalidInteger);
    }
    // The JSON number the host writes: integers keep their exact form.
    if value.fract() == 0.0 && value.abs() < i64::MAX as f64 {
        CoerceResult::Ok(serde_json::Value::from(value as i64))
    } else {
        CoerceResult::Ok(serde_json::from_str::<serde_json::Value>(text)
            .unwrap_or(serde_json::Value::from(value)))
    }
}

/// `NUMBER_RE` (`cellEditorKind.ts:168`):
/// `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$` — reimplemented
/// without a regex crate, member for member: an optional sign, then
/// either digits with an optional trailing `.` + digits, or a
/// leading `.` + digits, then an optional exponent.
fn number_re(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first().is_some_and(|b| *b == b'+' || *b == b'-') {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let digits = i - start;
    let mut fractional = 0;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let fstart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        fractional = i - fstart;
    }
    // `(\d+\.?\d*|\.\d+)`: either leading digits (then an optional
    // fraction), or a leading dot with digits.
    if digits == 0 && fractional == 0 {
        return false;
    }
    if digits == 0 && fractional > 0 {
        // `.5` — fine.
    }
    let mantissa = i;
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let estart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == estart {
            return false;
        }
    }
    i == bytes.len() && mantissa > 0
}

/// `coerceBoolean` (`cellEditorKind.ts:245-249`).
fn coerce_boolean(text: &str) -> CoerceResult {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized == "true" || normalized == "1" {
        return CoerceResult::Ok(serde_json::Value::Bool(true));
    }
    if normalized == "false" || normalized == "0" {
        return CoerceResult::Ok(serde_json::Value::Bool(false));
    }
    CoerceResult::Refused(CellValueErrorReason::InvalidBoolean)
}

/// `coerceTemporal` (`cellEditorKind.ts:251-288`): the date/time/datetime
/// branches with the calendar and clock checks.
fn coerce_temporal(input: &CoerceInput, text: &str) -> CoerceResult {
    if input.cell_type == CellType::Date {
        let Some((year, month, day)) = date_re(text) else {
            return CoerceResult::Refused(CellValueErrorReason::InvalidDate);
        };
        if !is_real_calendar_date(year, month, day) {
            return CoerceResult::Refused(CellValueErrorReason::InvalidDate);
        }
        // Wall clock: the text is the value.
        return CoerceResult::Ok(serde_json::Value::String(text.to_owned()));
    }
    if input.cell_type == CellType::Time {
        let Some((hour, minute, second)) = time_re(text) else {
            return CoerceResult::Refused(CellValueErrorReason::InvalidTime);
        };
        if !is_real_clock_time(hour, minute, second) {
            return CoerceResult::Refused(CellValueErrorReason::InvalidTime);
        }
        let canonical = format!("{hour:02}:{minute:02}:{second:02}");
        return CoerceResult::Ok(serde_json::Value::String(canonical));
    }
    // datetime without a zone.
    let Some((year, month, day, hour, minute, second)) = datetime_re(text) else {
        return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
    };
    if !is_real_calendar_date(year, month, day) {
        return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
    }
    if !is_real_clock_time(hour, minute, second) {
        return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
    }
    CoerceResult::Ok(serde_json::Value::String(text.to_owned()))
}

/// `coerceJson` (`cellEditorKind.ts:290-307`): invalid JSON is refused —
/// the old editor committed the raw text, which put a non-JSON string
/// into a JSON column.
fn coerce_json(text: &str) -> CoerceResult {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(parsed) => CoerceResult::Ok(parsed),
        Err(_) => CoerceResult::Refused(CellValueErrorReason::InvalidJson),
    }
}

/// `coerceText` (`cellEditorKind.ts:309-326`): the uuid and zoned
/// datetime checks; everything else passes through.
fn coerce_text(input: &CoerceInput, text: &str) -> CoerceResult {
    if input.cell_type == CellType::Uuid {
        let trimmed = text.trim();
        if !uuid_re(trimmed) {
            return CoerceResult::Refused(CellValueErrorReason::InvalidUuid);
        }
        return CoerceResult::Ok(serde_json::Value::String(trimmed.to_owned()));
    }
    if input.cell_type == CellType::DateTime {
        // A zoned column is edited as text: the offset must survive
        // untouched (`ZONED_DATETIME_RE`).
        let Some((year, month, day, hour, minute, second)) = zoned_datetime_re(text) else {
            return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
        };
        if !is_real_calendar_date(year, month, day) {
            return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
        }
        if !is_real_clock_time(hour, minute, second) {
            return CoerceResult::Refused(CellValueErrorReason::InvalidDateTime);
        }
        return CoerceResult::Ok(serde_json::Value::String(text.to_owned()));
    }
    CoerceResult::Ok(serde_json::Value::String(text.to_owned()))
}

/// `DATE_RE` (`\d{4}-\d{2}-\d{2}`) with the three captures.
fn date_re(text: &str) -> Option<(i32, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_digits(&bytes[0..4])?;
    let month = parse_digits(&bytes[5..7])?;
    let day = parse_digits(&bytes[8..10])?;
    Some((year, month as u32, day as u32))
}

/// `TIME_RE` (`\d{1,2}:\d{2}(?::\d{2})?`) with the three captures
/// (the optional second defaults to 0).
fn time_re(text: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let hour: u32 = parse_digits(parts[0].as_bytes())? as u32;
    if parts[0].len() < 1 || parts[0].len() > 2 {
        return None;
    }
    if parts[1].len() != 2 {
        return None;
    }
    let minute: u32 = parse_digits(parts[1].as_bytes())? as u32;
    let second: u32 = match parts.get(2) {
        Some(second) if second.len() == 2 => parse_digits(second.as_bytes())? as u32,
        Some(_) => return None,
        None => 0,
    };
    Some((hour, minute, second))
}

/// `DATETIME_RE`
/// (`\d{4}-\d{2}-\d{2}[T ]\d{1,2}:\d{2}(?::\d{2})?`) with the six
/// captures.
fn datetime_re(text: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let (date_part, time_part) = text.split_once(['T', ' '])?;
    let (year, month, day) = date_re(date_part)?;
    // The time half: `\d{1,2}:\d{2}(?::\d{2})?`.
    let parts: Vec<&str> = time_part.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 || parts[0].is_empty() || parts[0].len() > 2 {
        return None;
    }
    if parts[1].len() != 2 {
        return None;
    }
    let hour: u32 = parse_digits(parts[0].as_bytes())? as u32;
    let minute: u32 = parse_digits(parts[1].as_bytes())? as u32;
    let second: u32 = match parts.get(2) {
        Some(second) if second.len() == 2 => parse_digits(second.as_bytes())? as u32,
        Some(_) => return None,
        None => 0,
    };
    Some((year, month, day, hour, minute, second))
}

/// `ZONED_DATETIME_RE` (`DATETIME_RE` + an optional fraction and an
/// optional zone). The captures the checks read are the same six.
fn zoned_datetime_re(text: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    // Split the zone off first: the offset is optional and may be `Z`
    // or `[+-]\d{2}:?\d{2}`.
    let mut rest = text;
    let mut zone_len = 0;
    if let Some(at) = rest.rfind('Z') {
        if at + 1 == rest.len() {
            zone_len = 1;
            rest = &rest[..at];
        }
    }
    if zone_len == 0 {
        // `[+-]\d{2}:?\d{2}` at the end.
        let tail = rest.get(rest.len().saturating_sub(5)..).or_else(|| rest.get(rest.len().saturating_sub(4)..));
        if let Some(tail) = tail {
            let bytes = tail.as_bytes();
            let (offset_len, ok) = if bytes[0] == b'+' || bytes[0] == b'-' {
                if bytes.len() == 5 && bytes[3] == b':' {
                    (5, bytes[1..3].iter().all(u8::is_ascii_digit) && bytes[4].is_ascii_digit())
                } else if bytes.len() == 4 {
                    (4, bytes[1..].iter().all(u8::is_ascii_digit))
                } else {
                    (0, false)
                }
            } else {
                (0, false)
            };
            if ok {
                zone_len = offset_len;
                rest = &text[..text.len() - zone_len];
            }
        }
    } else {
        rest = &text[..text.len() - 1];
    }
    // An optional fractional second before the zone.
    if let Some(at) = rest.rfind('.') {
        let tail = &rest[at + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            rest = &rest[..at];
        }
    }
    datetime_re(rest)
}

/// `UUID_RE` (`[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}`,
/// case-insensitive).
fn uuid_re(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !(byte.is_ascii_hexdigit()) {
                    return false;
                }
            }
        }
    }
    true
}

/// `isRealCalendarDate` (`cellEditorKind.ts:190-198`): whether a
/// `YYYY-MM-DD` string is a real day of the calendar.
fn is_real_calendar_date(year: i32, month: u32, day: u32) -> bool {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    // The month's own length (leap February included) — the UTC date
    // round-trip the JS source performs, computed directly.
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let length = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    day <= length
}

/// `isRealClockTime` (`cellEditorKind.ts:203-205`).
fn is_real_clock_time(hour: u32, minute: u32, second: u32) -> bool {
    hour <= 23 && minute <= 59 && second <= 59
}

/// ASCII digits into an integer (the regex's `\d` — ASCII only).
fn parse_digits(bytes: &[u8]) -> Option<i32> {
    if bytes.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

