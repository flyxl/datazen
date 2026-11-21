use std::collections::BTreeMap;

use crate::IconResolverSeed;

pub const HOST_LUCIDE_MAP: [(&str, &str); 34] = [
    ("nav.settings", "Settings"),
    ("nav.databases", "Database"),
    ("nav.workspacePages", "LayoutGrid"),
    ("nav.extensions", "Puzzle"),
    ("query.run", "Play"),
    ("query.stop", "Square"),
    ("ai.chat", "Bot"),
    ("common.backupDatabase", "DatabaseBackup"),
    ("common.newQuery", "Code2"),
    ("common.newTable", "TableProperties"),
    ("common.erDiagram", "GitFork"),
    ("common.objects", "Code2"),
    ("action.sync", "Download"),
    ("action.refresh", "RefreshCcw"),
    ("common.newConnection", "Plus"),
    ("action.workflow", "Workflow"),
    ("action.dashboard", "Gauge"),
    ("action.serverStatus", "Activity"),
    ("action.processes", "Database"),
    ("action.privileges", "KeyRound"),
    ("theme.light", "Sun"),
    ("theme.dark", "Moon"),
    ("theme.system", "Monitor"),
    ("schema.database", "Database"),
    ("schema.schema", "FolderOpen"),
    ("schema.table", "Table2"),
    ("schema.view", "Eye"),
    ("schema.systemTable", "Table2"),
    ("schema.function", "Braces"),
    ("schema.procedure", "Braces"),
    ("schema.trigger", "Zap"),
    ("schema.sequence", "Hash"),
    ("schema.type", "Shapes"),
    ("schema.redisDatabase", "Database"),
];

pub const SETTINGS_SECTION_LUCIDE_MAP: [(&str, &str); 13] = [
    ("general", "Globe"),
    ("appearance", "Palette"),
    ("dataBrowsing", "Table2"),
    ("editor", "Code2"),
    ("behavior", "MousePointerClick"),
    ("logging", "FileText"),
    ("ai", "Bot"),
    ("prompts", "MessageSquareText"),
    ("mcpServer", "Server"),
    ("mcpClient", "Plug"),
    ("tunnels", "Cable"),
    ("monitor", "Activity"),
    ("extensions", "Puzzle"),
];

pub fn settings_section_icon_id(section: &str) -> String {
    if section == "ai" {
        "ai.chat".to_owned()
    } else {
        format!("settings.{section}")
    }
}

pub fn build_host_lucide_by_id() -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = HOST_LUCIDE_MAP
        .iter()
        .map(|(id, name)| ((*id).to_owned(), (*name).to_owned()))
        .collect();
    for (section, name) in SETTINGS_SECTION_LUCIDE_MAP {
        let id = settings_section_icon_id(section);
        if !map.contains_key(&id) {
            map.insert(id, name.to_owned());
        }
    }
    map
}

pub fn host_lucide_seed() -> IconResolverSeed {
    let mut seed = IconResolverSeed::default();
    seed.lucide_by_id = build_host_lucide_by_id();
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_map_is_complete_and_pinned() {
        assert_eq!(HOST_LUCIDE_MAP.len(), 34);
        let map: BTreeMap<&str, &str> = HOST_LUCIDE_MAP.iter().copied().collect();
        assert_eq!(map.len(), 34);
        assert_eq!(map["nav.settings"], "Settings");
        assert_eq!(map["query.run"], "Play");
        assert_eq!(map["common.objects"], "Code2");
        assert_eq!(map["action.processes"], "Database");
        assert_eq!(map["schema.trigger"], "Zap");
        assert_eq!(map["schema.redisDatabase"], "Database");
    }

    #[test]
    fn settings_sections_are_pinned() {
        assert_eq!(SETTINGS_SECTION_LUCIDE_MAP.len(), 13);
        let map: BTreeMap<&str, &str> = SETTINGS_SECTION_LUCIDE_MAP.iter().copied().collect();
        assert_eq!(map["general"], "Globe");
        assert_eq!(map["ai"], "Bot");
        assert_eq!(map["mcpServer"], "Server");
    }

    #[test]
    fn section_icon_id_specializes_ai() {
        assert_eq!(settings_section_icon_id("ai"), "ai.chat");
        assert_eq!(settings_section_icon_id("general"), "settings.general");
        assert_eq!(settings_section_icon_id("mcpClient"), "settings.mcpClient");
    }

    #[test]
    fn combined_map_adds_settings_without_overwriting_host_ids() {
        let map = build_host_lucide_by_id();
        assert_eq!(map.len(), 46);
        assert_eq!(map.get("ai.chat").map(String::as_str), Some("Bot"));
        assert_eq!(map.get("settings.general").map(String::as_str), Some("Globe"));
        assert_eq!(map.get("settings.ai"), None);
        assert_eq!(map.get("settings.extensions").map(String::as_str), Some("Puzzle"));
    }

    #[test]
    fn combined_map_feeds_the_icon_resolver_seed() {
        let seed = host_lucide_seed();
        assert_eq!(seed.lucide_by_id.len(), 46);
        assert_eq!(seed.pack_icons.len(), 0);
        assert_eq!(seed.driver_icons.len(), 0);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::settings_section_icon_id;
        let _ = crate::build_host_lucide_by_id;
        let _ = crate::host_lucide_seed;
        assert_eq!(crate::HOST_LUCIDE_MAP.len(), 34);
        assert_eq!(crate::SETTINGS_SECTION_LUCIDE_MAP.len(), 13);
    }
}
