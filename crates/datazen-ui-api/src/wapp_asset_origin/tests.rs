use super::*;

fn query(pairs: &[(&str, &str)]) -> WappAssetQueryMap {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn protocol_constants_are_pinned() {
    assert_eq!(WAPP_ASSET_PATH_PREFIX, "/wapps/");
    assert!(!WAPP_ASSET_CSP.contains("datazen:"));
    for directive in [
        "default-src 'self'",
        "script-src 'self'",
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data:",
        "font-src 'self'",
        "connect-src 'none'",
    ] {
        assert!(WAPP_ASSET_CSP.contains(directive));
    }
}

#[test]
fn wapp_id_grammar_is_exact() {
    for valid in ["a.bc", "a0.b-c", "acme.bill-audit", "0.abc"] {
        assert!(is_valid_wapp_id(valid), "{valid}");
    }
    for invalid in [
        "bill-audit",
        "Acme.bill-audit",
        "acme.",
        ".bill",
        "-lead.bill",
        "a.b.c",
        "a.X",
        "a.b_",
        "",
    ] {
        assert!(!is_valid_wapp_id(invalid), "{invalid}");
    }
}

#[test]
fn safe_paths_reject_empty_hidden_backslash_and_nul() {
    assert!(is_safe_wapp_asset_path("a/b/c/d.json"));
    assert!(is_safe_wapp_asset_path("open"));
    for unsafe_path in [
        "",
        "..",
        ".storage.json",
        ".enabled",
        "dir/.secret.png",
        "a\\b.html",
        "a\0b.html",
        "a//b",
        "/leading",
        "trailing/",
    ] {
        assert!(!is_safe_wapp_asset_path(unsafe_path), "{unsafe_path:?}");
    }
}

#[test]
fn both_resolvers_keep_their_protocol_shapes_and_dot_slash_normalization() {
    assert_eq!(
        (WEB_WAPP_ASSET_RESOLVER.build_src)("acme.bill-audit", "index.html", "1.0.0"),
        "/wapps/acme.bill-audit/index.html?v=1.0.0"
    );
    assert_eq!(
        (WEB_WAPP_ASSET_RESOLVER.build_icon_src)("acme.bill-audit", "assets/icon.svg"),
        "/wapps/acme.bill-audit/assets/icon.svg"
    );
    assert_eq!(
        (WEB_WAPP_ASSET_RESOLVER.build_src)("acme.bill-audit", "./main.html", "2.0.0"),
        "/wapps/acme.bill-audit/main.html?v=2.0.0"
    );
    assert_eq!(
        (DESKTOP_WAPP_ASSET_RESOLVER.build_src)("acme.bill-audit", "index.html", "1.0.0"),
        "datazen://acme.bill-audit/index.html?v=1.0.0"
    );
    assert_eq!(
        (DESKTOP_WAPP_ASSET_RESOLVER.build_icon_src)("acme.bill-audit", "./assets/icon.svg"),
        "datazen://acme.bill-audit/assets/icon.svg"
    );
}

#[test]
fn version_is_percent_encoded_as_a_uri_component() {
    assert_eq!(
        (WEB_WAPP_ASSET_RESOLVER.build_src)("acme.one", "i.html", "1.0.0+b"),
        "/wapps/acme.one/i.html?v=1.0.0%2Bb"
    );
    assert_eq!(
        (WEB_WAPP_ASSET_RESOLVER.build_src)("acme.one", "i.html", "a b/c"),
        "/wapps/acme.one/i.html?v=a%20b%2Fc"
    );
}

#[test]
fn the_assembly_time_binding_defaults_to_desktop_and_can_be_rebound() {
    reset_wapp_asset_resolver();
    assert_eq!(
        build_wapp_asset_src("acme.bill-audit", "index.html", "1.0.0"),
        "datazen://acme.bill-audit/index.html?v=1.0.0"
    );
    configure_wapp_asset_resolver(WEB_WAPP_ASSET_RESOLVER);
    assert_eq!(
        build_wapp_asset_src("acme.bill-audit", "index.html", "1.0.0"),
        "/wapps/acme.bill-audit/index.html?v=1.0.0"
    );
    assert_eq!(
        build_wapp_icon_src("acme.bill-audit", "assets/icon.svg"),
        "/wapps/acme.bill-audit/assets/icon.svg"
    );
    reset_wapp_asset_resolver();
    assert_eq!(
        build_wapp_icon_src("acme.bill-audit", "assets/icon.svg"),
        "datazen://acme.bill-audit/assets/icon.svg"
    );
}

#[test]
fn an_origin_relative_url_parses_id_path_and_query() {
    let target = match parse_wapp_asset_target("/wapps/acme.bill-audit/open?page=quota-check&uid=123") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(target.wapp_id, "acme.bill-audit");
    assert_eq!(target.path, "open");
    assert_eq!(target.query, query(&[("page", "quota-check"), ("uid", "123")]));
}

#[test]
fn decoding_precedes_validation_and_query_decoding_is_lenient() {
    let target = match parse_wapp_asset_target("/wapps/acme.bill-audit/assets%2Ficon.svg?q=a%20b") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(target.path, "assets/icon.svg");
    assert_eq!(target.query, query(&[("q", "a b")]));

    let single_plus = match parse_wapp_asset_target("/wapps/acme.one/open?q=a+b") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(single_plus.query, query(&[("q", "a b")]));

    let plus = match parse_wapp_asset_target("/wapps/acme.one/open?q=a+b&empty=&bare&q=last") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(
        plus.query,
        query(&[("q", "last"), ("empty", ""), ("bare", "")])
    );

    let malformed = match parse_wapp_asset_target("/wapps/acme.one/open?q=%zz%2B%2") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(malformed.query, query(&[("q", "%zz+%2")]));
}

#[test]
fn absolute_urls_and_extra_query_markers_follow_the_web_semantics() {
    let target = match parse_wapp_asset_target("https://app.example.com/wapps/acme.one/data.json") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(
        target,
        WappAssetTarget {
            wapp_id: "acme.one".to_owned(),
            path: "data.json".to_owned(),
            query: WappAssetQueryMap::new(),
        }
    );
    let with_port = match parse_wapp_asset_target("http://localhost:8080/wapps/acme.one/x?a=1") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(with_port.path, "x");

    let extra = match parse_wapp_asset_target("/wapps/acme.one/open?a=1?b=2") {
        Ok(value) => value,
        Err(error) => panic!("parse should succeed: {error}"),
    };
    assert_eq!(extra.query, query(&[("a", "1")]));
}

#[test]
fn the_same_inputs_the_desktop_parser_rejects_are_refused_here() {
    let unsupported = parse_wapp_asset_target("/plugins/acme.bill-audit/index.html");
    assert_eq!(
        unsupported.err().as_deref(),
        Some("unsupported wapp asset path: /plugins/acme.bill-audit/index.html")
    );
    for host in ["bill-audit", "Acme.bill-audit", "acme.", ".bill"] {
        let url = format!("/wapps/{host}/index.html");
        let error = parse_wapp_asset_target(&url).err().unwrap_or_default();
        assert_eq!(error, format!("invalid wapp host: `{host}`"));
    }
    let empty_path = parse_wapp_asset_target("/wapps/acme.bill-audit");
    assert_eq!(
        empty_path.err().as_deref(),
        Some("unsafe path component in ``")
    );
    for evil in [
        "/wapps/acme.bill-audit/%2e%2e/%2e%2e/settings.json",
        "/wapps/acme.bill-audit/.storage.json",
        "/wapps/acme.bill-audit/.enabled",
        "/wapps/acme.bill-audit/dir/.secret.png",
        "/wapps/acme.bill-audit/a%5Cb.html",
        "/wapps/acme.bill-audit/%00.html",
    ] {
        let error = parse_wapp_asset_target(evil).err().unwrap_or_default();
        assert!(error.starts_with("unsafe path component in"), "{evil}");
    }
}

#[test]
fn crate_root_reexports_the_asset_origin_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::WappAssetTarget>();
    resolves::<crate::WappAssetResolver>();
    resolves::<crate::WappAssetQueryMap>();
    assert_eq!(crate::WAPP_ASSET_PATH_PREFIX, "/wapps/");
    assert!(crate::is_valid_wapp_id("a.bc"));
    assert!(crate::is_safe_wapp_asset_path("a/b"));
    assert!(crate::parse_wapp_asset_target("/wapps/a.bc/x").is_ok());
}
