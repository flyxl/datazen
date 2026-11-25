//! T217: `HttpProxyTunnelFields` + `IndexConnectionFields` ported to Rune.
//!
//! Probes mount both field groups together and observe keys, labels and values
//! through the runtime element tree. The index clamp/blur rules are pure helpers
//! pinned directly, the way React's `onChange`/`onBlur` expressions are written.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::components::connection::http_proxy_tunnel_fields::{
    HttpProxyScheme, HttpProxyTunnelFields, HttpProxyValue,
};
use datazen_ui::components::connection::index_connection_fields::{
    clamp_index, default_index, IndexConnectionFields, IndexValue,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    http: HttpProxyValue,
    index: IndexValue,
    schemes: Vec<HttpProxyScheme>,
    dark: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            http: HttpProxyValue {
                host: TextEdit::new("proxy.corp.example".to_owned()),
                port: TextEdit::new("3128".to_owned()),
                scheme: HttpProxyScheme::Http,
                scheme_open: false,
                username: TextEdit::new(String::new()),
                password: TextEdit::new(String::new()),
                timeout: TextEdit::new("30".to_owned()),
            },
            index: IndexValue { database: TextEdit::new("3".to_owned()) },
            schemes: Vec::new(),
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let http = HttpProxyTunnelFields::<Scene>::new(
        &s.http,
        |st: &mut Scene| &mut st.http.host,
        |st: &mut Scene| &mut st.http.port,
        |st: &mut Scene| &mut st.http.username,
        |st: &mut Scene| &mut st.http.password,
        |st: &mut Scene| &mut st.http.timeout,
    )
    .scheme_open(|st: &mut Scene| &mut st.http.scheme_open)
    .on_scheme(|st: &mut Scene, scheme| st.schemes.push(scheme))
    .build(s.dark, &mut layer);
    let index = IndexConnectionFields::<Scene>::new(
        &s.index,
        |st: &mut Scene| &mut st.index.database,
    )
    .build(s.dark);
    col().w_full().h_full().child(http).child(index).children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {:?}: {outside:?}", outside.len(), size);
}

// --- 1. HTTP proxy fields ----------------------------------------------------

#[test]
fn http_proxy_fields_pin_all_testids_labels_and_values() {
    let rt = runtime(Scene::default(), DESIGN);
    assert!(has_key(&rt, "new-conn-http-proxy-fields"));
    for suffix in ["host", "port", "scheme", "timeout", "username", "password"] {
        let key = format!("new-conn-http-proxy-{suffix}");
        if suffix == "scheme" {
            assert!(has_key(&rt, &format!("{key}-trigger")), "{key}-trigger is mounted");
        } else {
            assert!(has_key(&rt, &key), "{key} is mounted");
        }
        assert!(has_key(&rt, &format!("{key}-label-text")), "{key}-label is mounted");
    }
    assert!(has_key(&rt, "new-conn-http-proxy-scheme-trigger"));
    assert_eq!(texts(&rt).iter().filter(|t| t.as_str() == "proxy.corp.example").count(), 1);
    assert!(texts(&rt).iter().any(|t| t == "3128"));
    assert!(texts(&rt).iter().any(|t| t == "30"));
    let hint = t("newConn.httpProxyHint");
    assert_eq!(texts(&rt).iter().filter(|text| text.as_str() == hint).count(), 1);
    assert_eq!(
        element(&rt, "new-conn-http-proxy-host-label-text").text.as_deref(),
        Some(t("newConn.httpProxyHost"))
    );
}

#[test]
fn scheme_select_offers_http_and_https_and_reports_the_choice() {
    let mut rt = runtime(Scene::default(), DESIGN);
    click(&mut rt, "new-conn-http-proxy-scheme-trigger");
    assert!(rt.state.http.scheme_open, "the trigger asks the host to open the listbox");
    assert_eq!(element(&rt, "option0").label.as_deref(), Some("HTTP"));
    assert_eq!(element(&rt, "option1").label.as_deref(), Some("HTTPS"));
    assert!(element(&rt, "option0").item_selected, "http is the current value");

    click(&mut rt, "option1");
    assert_eq!(rt.state.schemes, vec![HttpProxyScheme::Https], "the scheme callback is wired");
    assert!(!rt.state.http.scheme_open, "a choice closes the listbox");
    assert_eq!(HttpProxyScheme::from_value("https"), HttpProxyScheme::Https);
    assert_eq!(HttpProxyScheme::from_value("bogus"), HttpProxyScheme::Http);
}

// --- 2. index field ----------------------------------------------------------

#[test]
fn index_clamp_matches_reacts_parse_and_bounds() {
    assert_eq!(clamp_index(""), "", "an empty value stays empty while typing");
    assert_eq!(clamp_index("abc"), "0", "parseInt || 0");
    assert_eq!(clamp_index("12abc"), "12", "trailing garbage is ignored like parseInt");
    assert_eq!(clamp_index("-5"), "0", "the lower bound is clamped");
    assert_eq!(clamp_index("16"), "15", "the upper bound is clamped");
    assert_eq!(clamp_index("7"), "7");
    assert_eq!(clamp_index("99999999999999999999"), "15", "overflow still clamps high");
    assert_eq!(default_index(""), "0");
    assert_eq!(default_index("   "), "0");
    assert_eq!(default_index("7"), "7");
}

#[test]
fn index_field_has_the_localized_label_and_commits_an_empty_value_on_enter() {
    let mut scene = Scene::default();
    scene.index.database = TextEdit::new(String::new());
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "new-conn-database-index"));
    assert_eq!(
        element(&rt, "new-conn-database-index-label-text").text.as_deref(),
        Some(t("newConn.databaseIndex"))
    );

    let mut rt = rt;
    let (x, y) = rt.find("new-conn-database-index").unwrap_or_else(|| panic!("index field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.index.database.text, "0", "Enter is the expressible blur boundary");
}

// --- 3. geometry -------------------------------------------------------------

#[test]
fn both_field_groups_fit_both_window_sizes_and_both_palettes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(Scene { dark, ..Scene::default() }, size);
            assert!(has_key(&rt, "new-conn-http-proxy-fields"), "{size:?} dark={dark}: http root");
            assert!(has_key(&rt, "new-conn-database-index"), "{size:?} dark={dark}: index root");
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}
