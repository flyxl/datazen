//! Guard the zh-CN table the rune views now read through `i18n::t`.
//!
//! The table is generated from the React locale packs at build time, so this
//! module does not pin every string (that would duplicate the packs). It pins
//! the wiring: the table is populated, the keys the views took over resolve to
//! real Chinese text instead of falling back to the key, and `common.save`
//! stays the reference translation. There is deliberately no "no Chinese
//! literal" rule here: later tracks move more strings over step by step, and a
//! blanket ban would block work that is still in flight.

use crate::i18n::{len, t};

#[test]
fn the_zh_cn_table_is_generated_and_populated() {
    assert!(
        len() > 3_000,
        "the zh-CN table holds only {} entries",
        len()
    );
}

#[test]
fn keys_taken_over_by_the_rune_views_resolve() {
    // A sample of the keys the connection views now ask for. A typo or a pack
    // entry dropped in a later edit makes `t` fall back to the key itself, and
    // this assertion catches that here instead of showing a raw key in the UI.
    let keys = [
        "common.database",
        "common.importConnections",
        "common.newConnection",
        "connShare.changeImportFile",
        "import.selectFile",
        "newConn.advanced",
        "newConn.dbType",
        "newConn.password",
        "newConn.selectDbType",
        "newConn.sshHost",
        "newConn.sshPort",
        "newConn.sshUsername",
        "newConn.tunnel",
        "newConn.username",
    ];
    for key in keys {
        assert_ne!(t(key), key, "{key} falls back to the key");
    }
}

#[test]
fn the_reference_translation_is_still_chinese() {
    assert_eq!(t("common.save"), "保存");
}
