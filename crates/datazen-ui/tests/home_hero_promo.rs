//! T418: the standalone React `home/HomeHero.tsx` + `home/McpPromoBar.tsx`
//! ports. Observation is through `Runtime::elements()` (key / text / a11y
//! label / rect) and real pointer events — never by reading the VM back.
//!
//! Pinned: the `connections-*` keys the A1 home tests rely on, the 1:1 hero
//! counts and four-badge cap, the exact locale keys, a **real** 2000 ms copy
//! wake (`Frame::wake_deadline()`), and horizontal containment at both window
//! sizes in both appearances. Zero-size React testid markers are checked by
//! key presence only. The runtime state is `AppState` because the copy button
//! handler is registered for that type.

use rune::{col, tok, Cx, El, Env, Runtime, Tx};

use datazen_ui::app::app_poll;
use datazen_ui::i18n::t;
use datazen_ui::state_def::AppState;
use datazen_ui::views::home::home_hero::{build_home_hero, HomeHeroVm};
use datazen_ui::views::home::mcp_promo_bar::{
    build_mcp_promo_bar, McpPromoVm, MCP_COPIED_FEEDBACK_MS,
};

type HomeView = fn(&AppState, &mut Cx<AppState>) -> El;

const FIVE_TYPES: &[&str] = &["postgres", "mysql", "redis", "sqlite", "mongodb"];
const SIX_TYPES: &[&str] = &[
    "postgres", "mysql", "redis", "sqlite", "mongodb", "duckdb",
];

fn compose(s: &AppState, cx: &mut Cx<AppState>, db_types: &[&str]) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    col()
        .key("home-hero-promo-root")
        .w_full()
        .min_h(0.0)
        .px(24.0)
        .py(20.0)
        .gap(22.0)
        .child(
            build_home_hero(
                &HomeHeroVm {
                    connection_count: 5,
                    group_count: 2,
                    db_types: db_types.iter().map(|s| (*s).to_owned()).collect(),
                },
                theme,
                dark,
            ),
        )
        .child(build_mcp_promo_bar(
            &McpPromoVm::with_default_command(),
            s,
            theme,
            dark,
        ))
}

fn view_empty(s: &AppState, cx: &mut Cx<AppState>) -> El {
    compose(s, cx, &[])
}

fn view_five(s: &AppState, cx: &mut Cx<AppState>) -> El {
    compose(s, cx, FIVE_TYPES)
}

fn view_six(s: &AppState, cx: &mut Cx<AppState>) -> El {
    compose(s, cx, SIX_TYPES)
}

fn runtime(dark: bool, size: (f32, f32), view: HomeView) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        AppState::default(),
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn refresh(rt: &mut Runtime<AppState>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

// ---- Hero ---------------------------------------------------------------------------------------

#[test]
fn hero_copies_the_react_title_counts_and_caps_badges_at_four() {
    let rt = runtime(true, (1440.0, 816.0), view_six);

    for key in [
        "connections-home-hero",
        "connections-home-title",
        "connections-home-subtitle",
        "home-hero-subtitle",
        "connections-badge-cluster",
    ] {
        assert!(has(&rt, key), "{key}: A1/React hero key survives the port");
    }

    let rendered = texts(&rt);
    assert!(
        rendered.iter().any(|x| x == t("connWin.home.selectConnectionTitle")),
        "the hero title uses the exact selectConnectionTitle key"
    );
    let line = element(&rt, "connections-home-subtitle")
        .text
        .unwrap_or_default();
    for count in ["5", "2", "6"] {
        assert!(line.contains(count), "{count}: live count is in the hero: {line:?}");
    }
    for key in [
        "connWin.home.hero.subtitleConnections",
        "connWin.home.hero.subtitleGroups",
        "connWin.home.hero.subtitleTypes",
    ] {
        assert!(line.contains(t(key)), "{key}: exact key is in the hero: {line:?}");
    }

    let badges: Vec<&str> = SIX_TYPES
        .iter()
        .copied()
        .filter(|engine| has(&rt, &format!("home-hero-badge-{engine}")))
        .collect();
    assert_eq!(badges.len(), 4, "the badge cluster caps at four engine types");
    assert!(!has(&rt, "home-hero-badge-mongodb"), "the fifth engine is not rendered");
}

#[test]
fn hero_omits_the_badge_cluster_when_there_are_no_database_types() {
    let rt = runtime(false, (960.0, 640.0), view_empty);
    assert!(has(&rt, "connections-home-hero"), "hero still mounts");
    assert!(!has(&rt, "connections-badge-cluster"), "no types means no badge cluster");
    let line = element(&rt, "connections-home-subtitle")
        .text
        .unwrap_or_default();
    assert!(line.contains('0'), "the type count is zero: {line:?}");
}

// ---- MCP promo bar -----------------------------------------------------------------------------

#[test]
fn mcp_bar_shows_the_exact_icon_title_description_command_and_keys() {
    let rt = runtime(true, (960.0, 640.0), view_empty);
    assert!(has(&rt, "home-mcp-promo-bar"), "React promo-bar testid survives");
    assert!(has(&rt, "home-mcp-copy-button"), "React copy-button testid survives");

    let rendered = texts(&rt);
    assert!(
        rendered.iter().any(|x| x == "DataZen MCP Server"),
        "the hardcoded React promo title is shown verbatim"
    );
    assert!(
        rendered.iter().any(|x| x == "datazen --mcp"),
        "the default CLI command is shown"
    );
    for key in ["connWin.home.mcp.shortDesc", "connWin.home.aiIntegration.copy"] {
        assert!(
            rendered.iter().any(|x| x == t(key)),
            "{key}: exact locale key is shown"
        );
    }
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(t("connWin.home.aiIntegration.copy")),
        "the idle copy button uses the React aiIntegration.copy key"
    );
}

#[test]
fn mcp_copy_writes_the_command_and_arms_a_real_2000ms_wake() {
    assert_eq!(MCP_COPIED_FEEDBACK_MS, 2000);
    let clipboard = rune::clipboard::memory();
    let mut rt = runtime(true, (1440.0, 816.0), view_empty);
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    assert!(rt.wake_deadline().is_none(), "no wake before the copy press");

    let before = rt.clock.max(0.0);
    click(&mut rt, "home-mcp-copy-button");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("datazen --mcp"),
        "the copy button writes the exact CLI command to the real clipboard"
    );

    let until = rt
        .state
        .home_copied_until
        .expect("copy press records a deadline in host state");
    let deadline = rt
        .wake_deadline()
        .expect("copy press must schedule a real runtime wake");
    assert!(
        (deadline - until).abs() <= 0.01,
        "the wake must match the recorded deadline: wake={deadline} until={until}"
    );
    assert!(
        deadline - before > 1.95 && deadline - before < 2.10,
        "the copy wake must be ~2000ms after the press: before={before} wake={deadline}"
    );
    assert_eq!(
        rt.state.home_copied_id.as_deref(),
        Some("__mcp__"),
        "the bar-level copy marker is armed"
    );
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(t("connWin.home.aiIntegration.copied")),
        "the copied label shows inside the feedback window"
    );

    // The app poll reverts precisely at the deadline — not a boolean that
    // never expires. Poll with the pinned clock, then redraw.
    assert!(app_poll(&mut rt.state, until, &mut Tx::default()));
    refresh(&mut rt);
    assert!(rt.state.home_copied_until.is_none(), "deadline clears the marker");
    assert_eq!(
        element(&rt, "home-mcp-copy-button").label.as_deref(),
        Some(t("connWin.home.aiIntegration.copy")),
        "the label reverts to copy after the deadline"
    );
}

// ---- Geometry -----------------------------------------------------------------------------------

fn assert_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    assert!(has(rt, "home-hero-promo-root"), "{label}: probe root is mounted");
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        let key = e.key.unwrap_or(e.path);
        let key_is_root = key == "home-hero-promo-root";
        let horizontal = r[0] < -0.5 || r[0] + r[2] > w + 0.5;
        let vertical = r[1] < -0.5 || r[1] + r[3] > h + 0.5;
        if horizontal || (key_is_root && vertical) {
            outside.push(format!("{key} {r:?}"));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn hero_and_promo_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let empty = runtime(dark, (w, h), view_empty);
            assert_inside(&empty, w, h, &format!("empty dark={dark} {w}x{h}"));

            let with_badges = runtime(dark, (w, h), view_five);
            assert_inside(&with_badges, w, h, &format!("badges dark={dark} {w}x{h}"));

            let mut copied = runtime(dark, (w, h), view_empty);
            click(&mut copied, "home-mcp-copy-button");
            assert_inside(&copied, w, h, &format!("copied dark={dark} {w}x{h}"));
        }
    }
}
