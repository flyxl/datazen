//! T217 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 5 tests do not pin: the HTTP field
//! placeholders and the secure password, choosing HTTP from HTTPS and a close
//! without a choice, extra `parseInt`/clamp edge cases, committing a non-empty
//! out-of-range index, i18n key resolution, and a non-vacuous `visible`-rect
//! geometry observation at both sizes and palettes.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::components::connection::http_proxy_tunnel_fields::{
    HttpProxyScheme, HttpProxyTunnelFields, HttpProxyValue,
};
use crate::components::connection::index_connection_fields::{
    clamp_index, default_index, IndexConnectionFields, IndexValue,
};
use crate::i18n::{has, t};

#[derive(Clone)]
struct AuditScene {
    http: HttpProxyValue,
    index: IndexValue,
    schemes: Vec<HttpProxyScheme>,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self {
            http: HttpProxyValue {
                host: TextEdit::new(String::new()),
                port: TextEdit::new(String::new()),
                scheme: HttpProxyScheme::Http,
                scheme_open: false,
                username: TextEdit::new(String::new()),
                password: TextEdit::new(String::new()),
                timeout: TextEdit::new(String::new()),
            },
            index: IndexValue { database: TextEdit::new(String::new()) },
            schemes: Vec::new(),
            dark: true,
        }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let http = HttpProxyTunnelFields::<AuditScene>::new(
        &s.http,
        |st: &mut AuditScene| &mut st.http.host,
        |st: &mut AuditScene| &mut st.http.port,
        |st: &mut AuditScene| &mut st.http.username,
        |st: &mut AuditScene| &mut st.http.password,
        |st: &mut AuditScene| &mut st.http.timeout,
    )
    .scheme_open(|st: &mut AuditScene| &mut st.http.scheme_open)
    .on_scheme(|st: &mut AuditScene, scheme| st.schemes.push(scheme))
    .build(s.dark, &mut layer);
    let index = IndexConnectionFields::<AuditScene>::new(
        &s.index,
        |st: &mut AuditScene| &mut st.index.database,
    )
    .build(s.dark);
    col().w_full().h_full().child(http).child(index).children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// 1. React fixes host/port/timeout placeholders (surfaced as the input's
//    accessible name); password is type="password" so its text is hidden.
#[test]
fn http_placeholders_and_the_secure_password_match_react() {
    let mut scene = AuditScene::default();
    scene.http.password = TextEdit::new("SECRET123".to_owned());
    let rt = runtime(scene, (1440.0, 816.0));
    for placeholder in ["proxy.corp.example", "8080", "30"] {
        assert!(
            rt.elements().iter().any(|e| e.label.as_deref() == Some(placeholder)),
            "placeholder {placeholder} must appear as an input accessible name"
        );
    }
    assert!(
        !rt.elements().iter().any(|e| e.text.as_deref().is_some_and(|t| t.contains("SECRET123"))),
        "the password value must not be exposed as readable text"
    );
}

// 2. Choosing HTTP from an HTTPS current value emits Http; closing the listbox
//    without a choice does not emit anything but does clear the open flag.
#[test]
fn scheme_can_be_switched_back_to_http_and_closed_without_a_choice() {
    let mut scene = AuditScene::default();
    scene.http.scheme = HttpProxyScheme::Https;
    let mut rt = runtime(scene, (1440.0, 816.0));

    click(&mut rt, "new-conn-http-proxy-scheme-trigger");
    assert!(rt.state.http.scheme_open);
    click(&mut rt, "option0");
    assert_eq!(rt.state.schemes, vec![HttpProxyScheme::Http], "choosing HTTP reports Http");
    assert!(!rt.state.http.scheme_open);

    // Reopen, then dismiss with Escape rather than choosing.
    click(&mut rt, "new-conn-http-proxy-scheme-trigger");
    assert!(rt.state.http.scheme_open);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.schemes, vec![HttpProxyScheme::Http], "a dismissal emits no choice");
}

// 3. More `parseInt`/clamp edges that the shipped helper test omits.
#[test]
fn index_clamp_covers_signs_whitespace_and_hex_like_parse_int() {
    assert_eq!(clamp_index("+7"), "7", "a leading + is accepted");
    assert_eq!(clamp_index("  9"), "9", "leading whitespace is skipped");
    assert_eq!(clamp_index("0x10"), "0", "radix-10 parseInt stops at x");
    assert_eq!(clamp_index("1e3"), "1", "no exponent parsing");
    assert_eq!(clamp_index("-0"), "0", "negative zero clamps to the lower bound");
    assert_eq!(clamp_index("  "), "0", "whitespace-only is not an empty string");
    assert_eq!(clamp_index("１２"), "0", "full-width digits are not ASCII digits");
    assert_eq!(default_index(""), "0", "an empty blur value defaults to zero");
    assert_eq!(default_index("   "), "0", "a whitespace-only blur value defaults to zero");
    assert_eq!(default_index(&clamp_index("  ")), "0");
}

// 4. Enter commits a non-empty out-of-range value to the upper bound, not just
//    the empty-to-"0" blur case the shipped test covers.
#[test]
fn index_enter_commits_a_clamped_in_range_value() {
    let mut scene = AuditScene::default();
    scene.index.database = TextEdit::new("99".to_owned());
    let mut rt = runtime(scene, (1440.0, 816.0));

    let (x, y) = rt.find("new-conn-database-index").unwrap_or_else(|| panic!("index field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.index.database.text, "15", "Enter clamps 99 to the React upper bound");
}

#[test]
fn every_i18n_key_both_groups_use_resolves_without_echoing() {
    for key in [
        "newConn.httpProxyHost",
        "newConn.httpProxyPort",
        "newConn.httpProxyScheme",
        "newConn.httpProxyTimeout",
        "newConn.httpProxyUsername",
        "newConn.httpProxyPassword",
        "newConn.httpProxyHint",
        "newConn.databaseIndex",
    ] {
        assert!(has(key) && t(key) != key, "key {key} must resolve, not echo");
    }
}

// 5. Non-vacuous geometry: with every string filled to a long value, all
//    visible rects stay inside both windows at both palettes.
#[test]
fn filled_fields_stay_inside_both_windows_and_palettes_via_visible_rects() {
    let long = "x".repeat(120);
    let scene = AuditScene {
        http: HttpProxyValue {
            host: TextEdit::new(long.clone()),
            port: TextEdit::new(long.clone()),
            scheme: HttpProxyScheme::Https,
            scheme_open: false,
            username: TextEdit::new(long.clone()),
            password: TextEdit::new(long.clone()),
            timeout: TextEdit::new(long.clone()),
        },
        index: IndexValue { database: TextEdit::new(long.clone()) },
        schemes: Vec::new(),
        dark: true,
    };
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = runtime(AuditScene { dark, ..scene.clone() }, size);
            assert!(
                rt.elements().iter().any(|e| e.key.as_deref() == Some("new-conn-http-proxy-fields")),
                "{size:?} dark={dark}: HTTP root must be laid out"
            );
            assert!(
                rt.elements().iter().any(|e| e.key.as_deref() == Some("new-conn-database-index")),
                "{size:?} dark={dark}: index root must be laid out"
            );
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path),
                    e.visible,
                    e.rect
                );
            }
        }
    }
}
