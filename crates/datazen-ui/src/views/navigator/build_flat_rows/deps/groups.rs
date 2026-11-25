//! `lib/connectionGroups.ts` — the label half, ported for T570. Removal
//! stage: shard 2's T394 ports the whole file (its editor-facing halves
//! belong to the connection form); when it lands, this moves beside it.

use crate::i18n::t;

/// React `PRESET_GROUPS` (the three preset keys the navigator can meet).
const PRESET_GROUP_KEYS: [(&str, &str); 3] = [
    ("production", "newConn.groupProd"),
    ("development", "newConn.groupDev"),
    ("testing", "newConn.groupTest"),
];

/// React `formatGroupLabel` — a preset key gets its localized label; a custom
/// key is returned as-is. `t` falls back to the key itself, exactly as the
/// React test double does.
pub fn format_group_label(key: &str) -> String {
    for (preset, i18n_key) in PRESET_GROUP_KEYS {
        if key == preset {
            return t(i18n_key).to_owned();
        }
    }
    key.to_owned()
}
