//! T028 independent audit: `lib/chart/export.ts` port (`chart_export.rs`).
//!
//! Fresh probes, not a re-run of the coder's cases: new fixtures, boundary
//! inputs, and the two places where the Rust port is deliberately stricter
//! than the JS built-ins it replaces. Every behavioural claim below was
//! checked against Node's real `atob` / `decodeURIComponent` before it was
//! pinned here (see the comments).

use datazen_ui::support::chart_export::{
    chart_background, chart_svg_save_request, decode_svg_data_url, png_base64_from_data_url,
    png_save_request, resolve_background, resolve_png_capture_options, svg_save_request,
    ElementExportOptions, SvgDecodeError, CHART_SURFACE_DARK, CHART_SURFACE_LIGHT,
};

fn options(background_color: Option<&str>, quality: Option<f64>) -> ElementExportOptions {
    ElementExportOptions {
        background_color: background_color.map(str::to_owned),
        quality,
    }
}

#[test]
fn audit_default_options_resolve_to_dark_surface_with_no_quality() {
    // ElementExportOptions::default() is None/None: the capture gets the
    // dark chart surface and no quality key, as in React with no options.
    let resolved = resolve_png_capture_options(&ElementExportOptions::default(), true);
    assert_eq!(resolved.background_color, "#0b0e14");
    assert_eq!(resolved.quality, None);
    let resolved = resolve_png_capture_options(&ElementExportOptions::default(), false);
    assert_eq!(resolved.background_color, "#f6f8fb");
    assert_eq!(resolved.quality, None);
}

#[test]
fn audit_theme_values_are_distinct_and_match_themes_css() {
    // themes.css: :root (light) --c-surface #f6f8fb, .dark #0b0e14.
    assert_ne!(CHART_SURFACE_DARK, CHART_SURFACE_LIGHT);
    assert_eq!(chart_background(true), "#0b0e14");
    assert_eq!(chart_background(false), "#f6f8fb");
    assert_eq!(resolve_background(&options(None, None), false), "#f6f8fb");
    // An explicit colour wins in either theme.
    assert_eq!(
        resolve_background(&options(Some("#123456"), None), false),
        "#123456"
    );
}

#[test]
fn audit_quality_half_is_forwarded_and_png_only() {
    assert_eq!(
        resolve_png_capture_options(&options(None, Some(0.5)), true).quality,
        Some(0.5)
    );
    // The SVG path has no quality knob at all (React passes no quality to
    // toSvg); the SVG request builder takes no options struct.
    let request = svg_save_request("data:image/svg+xml,%3Csvg%3E", "x");
    assert!(request.is_ok());
}

#[test]
fn audit_png_split_boundary_inputs() {
    // Empty payload after the comma: split(',')[1] is "".
    assert_eq!(png_base64_from_data_url("data:image/png;base64,"), "");
    // A bare comma: segments are "" and "".
    assert_eq!(png_base64_from_data_url(","), "");
    // Empty input has no comma and passes through verbatim.
    assert_eq!(png_base64_from_data_url(""), "");
    // Only the segment after the FIRST comma is kept.
    assert_eq!(png_base64_from_data_url("a,b,c,d"), "b");
    // The file name is built even for an empty payload.
    assert_eq!(
        png_save_request("data:image/png;base64,", "empty").file_name,
        "empty.png"
    );
}

#[test]
fn audit_svg_base64_branch_wins_over_percent_escapes() {
    // Payload is base64 text that itself contains '%' characters once
    // decoded — the base64 branch must run, not the percent branch.
    // base64("%3Csvg%3E") == "JTNDc3ZnJTNF".
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,JTNDc3ZnJTNF"),
        Ok("%3Csvg%3E".to_owned())
    );
    // base64("<svg>%") == "PHN2Zz4l" (unpadded but length % 4 == 0: valid).
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,PHN2Zz4l"),
        Ok("<svg>%".to_owned())
    );
    // A comma inside the base64 payload is not base64 alphabet: atob throws,
    // and the port reports InvalidBase64 too.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=,extra"),
        Err(SvgDecodeError::InvalidBase64)
    );
}

#[test]
fn audit_svg_plain_branch_keeps_commas_and_literal_text() {
    // React slices after the FIRST comma; later commas pass through, and
    // decodeURIComponent leaves a literal ',' alone.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%3Csvg%3E,raw"),
        Ok("<svg>,raw".to_owned())
    );
    // %2C decodes to a comma.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%3Csvg%2C%3E"),
        Ok("<svg,>".to_owned())
    );
    // NUL and multi-byte UTF-8 round-trip (decodeURIComponent agrees).
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%00"),
        Ok("\0".to_owned())
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%C3%A9"),
        Ok("é".to_owned())
    );
    // Literal non-ASCII text passes through untouched.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,<svg>é</svg>"),
        Ok("<svg>é</svg>".to_owned())
    );
}

#[test]
fn audit_svg_verbatim_branch_requires_the_exact_prefix() {
    // A lookalike that does not START with the prefix is verbatim.
    assert_eq!(
        decode_svg_data_url("xdata:image/svg+xml,%3Csvg%3E"),
        Ok("xdata:image/svg+xml,%3Csvg%3E".to_owned())
    );
    // Empty payloads decode to empty text on both branches.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,"),
        Ok(String::new())
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,"),
        Ok(String::new())
    );
}

#[test]
fn audit_atob_whitespace_rules() {
    // atob strips ASCII whitespace (space, tab, LF, CR, FF) before decoding.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,PHN2\nZz48\tL3N2Zz4=\r"),
        Ok("<svg></svg>".to_owned())
    );
    // A non-breaking space is NOT ASCII whitespace: Node's atob throws
    // InvalidCharacterError on it, and the port reports InvalidBase64.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=\u{a0}"),
        Err(SvgDecodeError::InvalidBase64)
    );
}

#[test]
fn audit_unpadded_base64_is_rejected_where_atob_forgives() {
    // DIVERGENCE (non-blocking, malformed input only): Node's atob forgives
    // missing padding — atob("QQ") === "A", atob("QUE") === "AA" — while
    // the port's STANDARD engine requires canonical padding and reports
    // InvalidBase64. Real html-to-image output is always padded, so no real
    // capture can hit this; the strictness is pinned here so it stays a
    // deliberate choice rather than silent drift.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,QQ"),
        Err(SvgDecodeError::InvalidBase64)
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,QUE"),
        Err(SvgDecodeError::InvalidBase64)
    );
    // Length % 4 == 1 throws in atob too, so both agree here.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,Q"),
        Err(SvgDecodeError::InvalidBase64)
    );
}

#[test]
fn audit_non_utf8_base64_is_an_error_where_atob_succeeds() {
    // DIVERGENCE (non-blocking, malformed input only): atob("/w==") returns
    // the Latin-1 string "\xff" without throwing, so React would hand a
    // mojibake string to the save dialog; the port reports InvalidUtf8
    // instead. Erroring is the safer behaviour for an SVG text payload.
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml;base64,/w=="),
        Err(SvgDecodeError::InvalidUtf8)
    );
    // A lone 0xFF via the percent branch is InvalidUtf8 in both worlds
    // (decodeURIComponent throws URIError there).
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%FF"),
        Err(SvgDecodeError::InvalidUtf8)
    );
}

#[test]
fn audit_percent_decode_rejects_truncated_and_bad_escapes() {
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%2"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%2G"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,abc%"),
        Err(SvgDecodeError::InvalidPercentEncoding)
    );
    // Incomplete UTF-8 sequence: %C3 alone is an error, %C3%A9 is "é".
    assert_eq!(
        decode_svg_data_url("data:image/svg+xml,%C3"),
        Err(SvgDecodeError::InvalidUtf8)
    );
}

#[test]
fn audit_dialog_options_pin_name_filter_and_empty_title() {
    let png = png_save_request("data:image/png;base64,QUJD", "audit-chart");
    let dialog = png.dialog_options();
    assert_eq!(dialog.file_name.as_deref(), Some("audit-chart.png"));
    assert_eq!(
        dialog.filters,
        vec![("PNG".to_owned(), vec!["png".to_owned()])]
    );
    // No title is set here: the module docs leave the title to the caller
    // (precedent: begin_export_form sets its own "Export …" title).
    assert_eq!(dialog.title, "");

    let svg = svg_save_request("data:image/svg+xml,%3Csvg%3E", "audit-chart")
        .expect("audit SVG fixture decodes");
    let dialog = svg.dialog_options();
    assert_eq!(dialog.file_name.as_deref(), Some("audit-chart.svg"));
    assert_eq!(
        dialog.filters,
        vec![("SVG".to_owned(), vec!["svg".to_owned()])]
    );
    assert_eq!(dialog.title, "");
}

#[test]
fn audit_chart_svg_request_passes_serialized_text_through() {
    // Whitespace-only "serialization" is still Some text, not an absence.
    let request = chart_svg_save_request(Some("  "), "ws");
    assert_eq!(
        request.map(|request| (request.svg_text, request.file_name)),
        Some(("  ".to_owned(), "ws.svg".to_owned()))
    );
    assert_eq!(chart_svg_save_request(None, "ws"), None);
}

#[test]
fn audit_svg_save_request_error_keeps_no_file_name() {
    // The builder fails as a whole on a bad payload: there is no partial
    // request with a file name attached.
    assert_eq!(
        svg_save_request("data:image/svg+xml;base64,!!!", "bad"),
        Err(SvgDecodeError::InvalidBase64)
    );
}
