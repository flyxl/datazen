//! Chart / element export helpers, ported from
//! `packages/frontend/src/lib/chart/export.ts`.
//!
//! The React module has two halves: pure payload shaping (options, data-URL
//! splitting/decoding, file names, dialog filters) and DOM/host effects
//! (html-to-image capture, `XMLSerializer`, the atomic save dialog). This
//! module ports the first half 1:1 and defines the contract the second half
//! must satisfy — it does not invent a host API for it.
//!
//! ## What maps 1:1
//!
//! * [`ElementExportOptions`] — `backgroundColor` (default: the chart surface
//!   colour) and PNG-only `quality` (0–1). Quality reaches the capture only
//!   when explicitly set; [`resolve_png_capture_options`] keeps it `None`
//!   otherwise, mirroring the React conditional spread
//!   `...(quality !== undefined ? { quality } : {})`. An explicit `0.0` is a
//!   real quality, not an absence.
//! * [`png_base64_from_data_url`] — the PNG data URL is split at the first
//!   comma (`split(',')[1]`); a data URL with no comma is saved verbatim.
//! * [`decode_svg_data_url`] — the SVG three-way decode, in React order:
//!   `data:image/svg+xml;base64,` → base64-decode; `data:image/svg+xml,` →
//!   percent-decode after the first comma; anything else → verbatim.
//!   Decoding failures are `Err` here where the React `async` functions
//!   reject (`atob` / `decodeURIComponent` throwing); see
//!   [`SvgDecodeError`].
//! * [`chart_svg_save_request`] — `None` when there is no `<svg>` to
//!   serialize, i.e. no dialog opens, mirroring the React early return.
//!
//! ## Why chart SVG and element SVG stay separate
//!
//! Deliberately not unified, as in React: a chart element **is** a single
//! `<svg>`, so serializing it is exact, while the ER diagram's viewport is
//! HTML nodes plus SVG edges and needs html-to-image's `toSvg` to capture its
//! HTML nodes. Same save-dialog call, different rasterizer —
//! [`chart_svg_save_request`] takes an already-serialized `<svg>` document,
//! [`svg_save_request`] takes a `toSvg` data URL.
//!
//! ## The host half (not ported here — read before wiring a caller)
//!
//! Rune has no DOM, so the capture side has no Rune equivalent:
//!
//! * `html-to-image` `toPng` / `toSvg` (element capture, PNG rasterization)
//!   and `XMLSerializer` (chart `<svg>` serialization) cannot run in the
//!   native UI. The host must supply the capture: a PNG/SVG data URL for an
//!   element, or a serialized `<svg>` document for a chart. The request
//!   builders in this module define the payload contract the capture consumes
//!   ([`PngCaptureOptions`]: resolved background + optional quality) and
//!   produces ([`PngSaveRequest`] / [`SvgSaveRequest`]).
//! * The save dialog: React's `saveBase64WithDialog` / `saveTextWithDialog`
//!   are atomic dialog+write Tauri commands (`save_base64_with_dialog` /
//!   `save_text_with_dialog` in `crates/datazen-desktop/src/commands/file.rs`;
//!   the path never reaches JS). Rune's [`rune::Tx::save_file`] only returns
//!   the chosen path, and `DesktopAdapter` has no file-write method today, so
//!   a caller must pair the dialog answer with a host write. The
//!   `dialog_options()` constructors on the save requests pin the file name
//!   (`{filename}.png` / `{filename}.svg`) and filter (`PNG` / `png`,
//!   `SVG` / `svg`) the dialog must use; the title is the caller's choice
//!   (precedent: `begin_export_form` in `connections_io_state.rs`).
//! * The background default: `readCssColor` (`packages/frontend/src/lib/chart/colors.ts`)
//!   has no Rune port yet — there is no CSS variable to read without a DOM. Until
//!   it lands, [`chart_background`] pins the two theme values from
//!   `packages/frontend/src/styles/themes.css` (`--c-surface`: `#0b0e14`
//!   dark / `#f6f8fb` light); the caller supplies dark/light from app state.
//!   The React fallback `#0b0e14` is the dark value, so dark is also the
//!   value an unset variable resolves to.
//!
#![allow(dead_code)] // consumers: ChartToolbar / ChartView Rune ports (not yet landed)

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rune::FileDialogOptions;

/// The one DOM→PNG / DOM→SVG option set, mirroring React's
/// `ElementExportOptions`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ElementExportOptions {
    /// Background painted behind the element; defaults to the chart surface
    /// colour ([`chart_background`]).
    pub background_color: Option<String>,
    /// PNG only: capture output quality (0–1). `None` means "omit" — the
    /// capture uses its own default.
    pub quality: Option<f64>,
}

/// The resolved options handed to the PNG capture: the background is always
/// present, the quality only when the caller set one explicitly.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PngCaptureOptions {
    pub background_color: String,
    pub quality: Option<f64>,
}

/// A PNG payload ready for the save dialog: base64 bytes plus the file name
/// the dialog must suggest.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PngSaveRequest {
    pub base64: String,
    pub file_name: String,
}

/// An SVG payload ready for the save dialog: decoded SVG text plus the file
/// name the dialog must suggest.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SvgSaveRequest {
    pub svg_text: String,
    pub file_name: String,
}

/// Filter name for the PNG save dialog (React `'PNG'`).
pub(crate) const PNG_FILTER_NAME: &str = "PNG";
/// Extensions for the PNG save dialog (React `['png']`).
pub(crate) const PNG_EXTENSIONS: &[&str] = &["png"];
/// Filter name for the SVG save dialog (React `'SVG'`).
pub(crate) const SVG_FILTER_NAME: &str = "SVG";
/// Extensions for the SVG save dialog (React `['svg']`).
pub(crate) const SVG_EXTENSIONS: &[&str] = &["svg"];

/// Chart surface colour, dark theme (`--c-surface` in `themes.css`).
pub(crate) const CHART_SURFACE_DARK: &str = "#0b0e14";
/// Chart surface colour, light theme (`--c-surface` in `themes.css`).
pub(crate) const CHART_SURFACE_LIGHT: &str = "#f6f8fb";

/// The chart surface colour for the current theme — the Rune-side stand-in
/// for `readCssColor('--c-surface', '#0b0e14')` (see the module docs).
pub(crate) fn chart_background(dark: bool) -> &'static str {
    if dark {
        CHART_SURFACE_DARK
    } else {
        CHART_SURFACE_LIGHT
    }
}

/// The background the capture paints: the caller's colour, or the chart
/// surface when none was given. An explicit empty string passes through —
/// React's `??` only fills `null`/`undefined`, not `""`.
pub(crate) fn resolve_background(options: &ElementExportOptions, dark: bool) -> String {
    match &options.background_color {
        Some(color) => color.clone(),
        None => chart_background(dark).to_owned(),
    }
}

/// Resolve the PNG capture options: background always filled, quality carried
/// only when explicitly set.
pub(crate) fn resolve_png_capture_options(
    options: &ElementExportOptions,
    dark: bool,
) -> PngCaptureOptions {
    PngCaptureOptions {
        background_color: resolve_background(options, dark),
        quality: options.quality,
    }
}

/// The file name the PNG save dialog suggests (`{filename}.png`).
pub(crate) fn png_file_name(filename: &str) -> String {
    format!("{filename}.png")
}

/// The file name the SVG save dialog suggests (`{filename}.svg`).
pub(crate) fn svg_file_name(filename: &str) -> String {
    format!("{filename}.svg")
}

/// Split a PNG data URL to its base64 payload: the segment after the first
/// comma — exactly React's `dataUrl.split(',')[1]`. A data URL with no comma
/// is returned verbatim. (Base64 never contains `,`, so real captures never
/// hit the multi-comma case; the `[1]` index is pinned for parity anyway.)
pub(crate) fn png_base64_from_data_url(data_url: &str) -> &str {
    match data_url.split(',').nth(1) {
        Some(base64) => base64,
        None => data_url,
    }
}

/// Build the PNG save request from a capture data URL.
pub(crate) fn png_save_request(data_url: &str, filename: &str) -> PngSaveRequest {
    PngSaveRequest {
        base64: png_base64_from_data_url(data_url).to_owned(),
        file_name: png_file_name(filename),
    }
}

/// Why an SVG data URL could not be decoded. React's `atob` /
/// `decodeURIComponent` throw here; this module reports instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SvgDecodeError {
    /// The base64 payload is not valid standard base64 (`atob` throws).
    InvalidBase64,
    /// A `%` escape is malformed (`decodeURIComponent` throws).
    InvalidPercentEncoding,
    /// The decoded bytes are not valid UTF-8 (`decodeURIComponent` throws).
    InvalidUtf8,
}

impl std::fmt::Display for SvgDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SvgDecodeError::InvalidBase64 => f.write_str("invalid base64 in SVG data URL"),
            SvgDecodeError::InvalidPercentEncoding => {
                f.write_str("invalid percent escape in SVG data URL")
            }
            SvgDecodeError::InvalidUtf8 => f.write_str("decoded SVG data URL is not valid UTF-8"),
        }
    }
}

impl std::error::Error for SvgDecodeError {}

/// One hex digit's value, or `None` for a non-hex byte.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// JS `decodeURIComponent` on the byte level: `%XX` becomes one byte, every
/// other byte passes through (`+` stays `+`). Malformed escapes are an
/// error; UTF-8 validation happens in the caller.
fn percent_decode_to_bytes(input: &str) -> Result<Vec<u8>, SvgDecodeError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = bytes
                .get(index + 1)
                .and_then(|byte| hex_value(*byte))
                .ok_or(SvgDecodeError::InvalidPercentEncoding)?;
            let lo = bytes
                .get(index + 2)
                .and_then(|byte| hex_value(*byte))
                .ok_or(SvgDecodeError::InvalidPercentEncoding)?;
            out.push((hi << 4) | lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Ok(out)
}

/// Decode an SVG data URL to its text, in React's priority order:
///
/// 1. `data:image/svg+xml;base64,…` → base64-decode (JS `atob`: ASCII
///    whitespace is stripped first, then standard base64 is required);
/// 2. `data:image/svg+xml,…` → percent-decode the part after the first comma
///    (JS `decodeURIComponent`);
/// 3. anything else → the input verbatim.
pub(crate) fn decode_svg_data_url(data_url: &str) -> Result<String, SvgDecodeError> {
    const BASE64_PREFIX: &str = "data:image/svg+xml;base64,";
    const PLAIN_PREFIX: &str = "data:image/svg+xml,";
    if let Some(payload) = data_url.strip_prefix(BASE64_PREFIX) {
        // `atob` strips ASCII whitespace before decoding.
        let cleaned: String = payload
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect();
        let bytes = STANDARD
            .decode(cleaned)
            .map_err(|_| SvgDecodeError::InvalidBase64)?;
        return String::from_utf8(bytes).map_err(|_| SvgDecodeError::InvalidUtf8);
    }
    if let Some(payload) = data_url.strip_prefix(PLAIN_PREFIX) {
        let bytes = percent_decode_to_bytes(payload)?;
        return String::from_utf8(bytes).map_err(|_| SvgDecodeError::InvalidUtf8);
    }
    Ok(data_url.to_owned())
}

/// Build the SVG save request from a capture data URL.
pub(crate) fn svg_save_request(
    data_url: &str,
    filename: &str,
) -> Result<SvgSaveRequest, SvgDecodeError> {
    Ok(SvgSaveRequest {
        svg_text: decode_svg_data_url(data_url)?,
        file_name: svg_file_name(filename),
    })
}

/// Build the chart-SVG save request from an already-serialized `<svg>`
/// document (the `XMLSerializer` output in React). `None` — the chart has no
/// `<svg>` — means no save request and no dialog, mirroring the React early
/// return. This is deliberately a different entry point from
/// [`svg_save_request`]: see the module docs.
pub(crate) fn chart_svg_save_request(
    serialized_svg: Option<&str>,
    filename: &str,
) -> Option<SvgSaveRequest> {
    serialized_svg.map(|svg| SvgSaveRequest {
        svg_text: svg.to_owned(),
        file_name: svg_file_name(filename),
    })
}

/// The save-dialog options for a PNG request: the request's file name and the
/// `PNG` / `png` filter. Requesting the dialog (`Tx::save_file`) and writing
/// the bytes at the answered path remain the caller's job — see the module
/// docs.
fn save_dialog_options(
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> FileDialogOptions {
    FileDialogOptions {
        file_name: Some(file_name.to_owned()),
        ..Default::default()
    }
    .filter(filter_name, extensions)
}

impl PngSaveRequest {
    /// The save-dialog options for this request.
    pub(crate) fn dialog_options(&self) -> FileDialogOptions {
        save_dialog_options(&self.file_name, PNG_FILTER_NAME, PNG_EXTENSIONS)
    }
}

impl SvgSaveRequest {
    /// The save-dialog options for this request.
    pub(crate) fn dialog_options(&self) -> FileDialogOptions {
        save_dialog_options(&self.file_name, SVG_FILTER_NAME, SVG_EXTENSIONS)
    }
}
