//! T028: `lib/chart/export.ts` port — options resolution, data-URL
//! splitting/decoding, save-request building and dialog options.
//!
//! Mirrors `packages/frontend/src/lib/chart/__tests__/export.test.ts`: the
//! React tests mock `html-to-image` and the file commands, so what they pin
//! is exactly the pure half ported here — the options handed to the capture,
//! the payload handed to the save dialog, and the file names / filters.

use crate::support::chart_export::{
    chart_background, chart_svg_save_request, decode_svg_data_url, png_base64_from_data_url,
    png_save_request, resolve_background, resolve_png_capture_options, svg_save_request,
    ElementExportOptions, PngCaptureOptions, SvgDecodeError, CHART_SURFACE_DARK,
    CHART_SURFACE_LIGHT, PNG_EXTENSIONS, PNG_FILTER_NAME, SVG_EXTENSIONS, SVG_FILTER_NAME,
};

fn options(background_color: Option<&str>, quality: Option<f64>) -> ElementExportOptions {
    ElementExportOptions {
        background_color: background_color.map(str::to_owned),
        quality,
    }
}

#[test]
fn chart_background_matches_theme_surface() {
    assert_eq!(chart_background(true), CHART_SURFACE_DARK);
    assert_eq!(chart_background(true), "#0b0e14");
    assert_eq!(chart_background(false), CHART_SURFACE_LIGHT);
    assert_eq!(chart_background(false), "#f6f8fb");
}

#[test]
fn png_capture_options_forward_explicit_background_and_quality() {
    // React: exportElementAsPng(el, 'er-diagram-db', { backgroundColor: '#1a1a2e', quality: 1 })
    // hands toPng { backgroundColor: '#1a1a2e', quality: 1 } (ER diagram path).
    assert_eq!(
        resolve_png_capture_options(&options(Some("#1a1a2e"), Some(1.0)), true),
        PngCaptureOptions {
            background_color: "#1a1a2e".to_owned(),
            quality: Some(1.0),
        }
    );
}

#[test]
fn png_capture_options_default_to_chart_surface_when_no_background() {
    // React: no background ⇒ toPng gets the live --c-surface value (#0b1220
    // in the test's document); here the theme value for the dark default.
    assert_eq!(
        resolve_png_capture_options(&options(None, None), true).background_color,
        "#0b0e14"
    );
    assert_eq!(
        resolve_png_capture_options(&options(None, None), false).background_color,
        "#f6f8fb"
    );
}

#[test]
fn png_capture_options_omit_quality_unless_explicit() {
    // The conditional spread: no quality ⇒ no `quality` key at all.
    assert_eq!(
        resolve_png_capture_options(&options(None, None), true).quality,
        None
    );
    // An explicit 0.0 is a real quality, not an absence (React checks
    // `!== undefined`, not truthiness).
    assert_eq!(
        resolve_png_capture_options(&options(None, Some(0.0)), true).quality,
        Some(0.0)
    );
}

#[test]
fn explicit_empty_background_passes_through() {
    // React's `??` only fills null/undefined; "" is a caller's colour.
    assert_eq!(resolve_background(&options(Some(""), None), true), "");
}

#[test]
fn png_save_request_splits_data_url_to_base64() {
    // React: exportChartAsPng(el, 'chart-1') with toPng →
    // 'data:image/png;base64,QUJD' saves 'QUJD' as 'chart-1.png' ('PNG', ['png']).
    let request = png_save_request("data:image/png;base64,QUJD", "chart-1");
    assert_eq!(request.base64, "QUJD");
    assert_eq!(request.file_name, "chart-1.png");
    let dialog = request.dialog_options();
    assert_eq!(dialog.file_name.as_deref(), Some("chart-1.png"));
    assert_eq!(
        dialog.filters,
        vec![(
            PNG_FILTER_NAME.to_owned(),
            PNG_EXTENSIONS.iter().map(|ext| (*ext).to_owned()).collect()
        )]
    );
}

#[test]
fn png_save_request_keeps_data_url_without_comma_verbatim() {
    assert_eq!(png_base64_from_data_url("rawbase64"), "rawbase64");
    assert_eq!(png_save_request("rawbase64", "chart-1").base64, "rawbase64");
}

#[test]
fn png_base64_split_matches_js_split_index_1() {
    // `split(',')[1]` takes only the segment after the first comma; base64
    // never contains ',', so real captures never hit this, but the parity is
    // pinned so a future "rest of string" rewrite cannot drift silently.
    assert_eq!(png_base64_from_data_url("data:x,y,z"), "y");
    assert_eq!(
        png_base64_from_data_url("data:image/png;base64,QUJD"),
        "QUJD"
    );
}

#[test]
fn svg_save_request_decodes_base64_data_url() {
    // React: exportElementAsSvg(el, 'er-diagram-db', { backgroundColor: '#1a1a2e' })
    // with toSvg → 'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=' saves '<svg></svg>'
    // as 'er-diagram-db.svg' ('SVG', ['svg']).
    let request = svg_save_request(
        "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=",
        "er-diagram-db",
    );
    assert_eq!(
        request.as_ref().map(|request| request.svg_text.clone()),
        Ok("<svg></svg>".to_owned())
    );
    let request = request.expect("base64 SVG fixture decodes");
    assert_eq!(request.file_name, "er-diagram-db.svg");
    let dialog = request.dialog_options();
    assert_eq!(dialog.file_name.as_deref(), Some("er-diagram-db.svg"));
    assert_eq!(
        dialog.filters,
        vec![(
            SVG_FILTER_NAME.to_owned(),
            SVG_EXTENSIONS.iter().map(|ext| (*ext).to_owned()).collect()
        )]
    );
    // `atob` strips ASCII whitespace before decoding, so a spaced payload
    // still decodes.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,PHN2 Zz48L3N2Zz4="),
        Ok("<svg></svg>".to_owned())
    );
}

#[test]
fn svg_save_request_decodes_percent_encoded_data_url() {
    assert_eq!(
        svg_save_request("data:image/svg+xml,%3Csvg%3E%3C%2Fsvg%3E", "er-diagram-db")
            .map(|request| request.svg_text),
        Ok("<svg></svg>".to_owned())
    );
    // '+' is not a space for decodeURIComponent; '%' escapes are case-insensitive.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%3csvg%3E+%3C%2Fsvg%3E"),
        Ok("<svg>+</svg>".to_owned())
    );
}

#[test]
fn svg_save_request_keeps_non_data_url_verbatim() {
    assert_eq!(
        svg_save_request("<svg>raw</svg>", "er-diagram-db").map(|request| request.svg_text),
        Ok("<svg>raw</svg>".to_owned())
    );
}

#[test]
fn svg_decode_rejects_invalid_base64() {
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,!!!"),
        Err(SvgDecodeError::InvalidBase64)
    );
    // Missing padding is invalid too (atob throws on a length that is not a
    // multiple of 4 after whitespace stripping).
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,QQ"),
        Err(SvgDecodeError::InvalidBase64)
    );
    // The error also propagates out of the save-request builder (the `?`).
    assert_eq!(
        svg_save_request("data:image/svg+xml;base64,!!!", "er-diagram-db"),
        Err(SvgDecodeError::InvalidBase64)
    );
}

#[test]
fn svg_decode_rejects_malformed_percent_escape() {
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%zz"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%1"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
}

#[test]
fn svg_decode_rejects_invalid_utf8() {
    // Base64 of a lone 0xFF byte: decodes, but is not UTF-8 text.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,/w=="),
        Err(SvgDecodeError::InvalidUtf8)
    );
    // Percent-decoding %C3%28 yields an invalid UTF-8 sequence too.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%C3%28"),
        Err(SvgDecodeError::InvalidUtf8)
    );
}

#[test]
fn svg_decode_error_messages_are_stable() {
    assert_eq!(
        SvgDecodeError::InvalidBase64.to_string(),
        "invalid base64 in SVG data URL"
    );
    assert_eq!(
        SvgDecodeError::InvalidPercentEncoding.to_string(),
        "invalid percent escape in SVG data URL"
    );
    assert_eq!(
        SvgDecodeError::InvalidUtf8.to_string(),
        "decoded SVG data URL is not valid UTF-8"
    );
}

#[test]
fn chart_svg_save_request_serializes_given_svg() {
    // React: an inner <svg> serializes and saves via the dialog.
    let svg = "<svg><rect width=\"10\" height=\"10\"/></svg>";
    let request = chart_svg_save_request(Some(svg), "chart-svg");
    assert_eq!(
        request.as_ref().map(|request| request.svg_text.clone()),
        Some(svg.to_owned())
    );
    assert_eq!(
        request.map(|request| request.file_name),
        Some("chart-svg.svg".to_owned())
    );
}

#[test]
fn chart_svg_save_request_is_none_without_svg() {
    // React: no <svg> ⇒ return early, no dialog opens.
    assert_eq!(chart_svg_save_request(None, "empty"), None);
}

#[test]
fn chart_svg_save_request_keeps_empty_svg_text() {
    // Some("") is a serialized (if degenerate) document, not an absence.
    assert_eq!(
        chart_svg_save_request(Some(""), "empty").map(|request| request.svg_text),
        Some(String::new())
    );
}
