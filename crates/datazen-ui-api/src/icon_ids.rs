use crate::HOST_LUCIDE_MAP;

pub type UiIconId = &'static str;

pub const UI_ICON_IDS: [UiIconId; 34] = [
    "nav.settings",
    "nav.databases",
    "nav.workspacePages",
    "nav.extensions",
    "query.run",
    "query.stop",
    "ai.chat",
    "common.backupDatabase",
    "common.newQuery",
    "common.newTable",
    "common.erDiagram",
    "common.objects",
    "action.sync",
    "action.refresh",
    "common.newConnection",
    "action.workflow",
    "action.dashboard",
    "action.serverStatus",
    "action.processes",
    "action.privileges",
    "theme.light",
    "theme.dark",
    "theme.system",
    "schema.database",
    "schema.schema",
    "schema.table",
    "schema.view",
    "schema.systemTable",
    "schema.function",
    "schema.procedure",
    "schema.trigger",
    "schema.sequence",
    "schema.type",
    "schema.redisDatabase",
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn icon_ids_are_pinned_and_unique() {
        assert_eq!(UI_ICON_IDS.len(), 34);
        let set: BTreeSet<UiIconId> = UI_ICON_IDS.iter().copied().collect();
        assert_eq!(set.len(), 34);
    }

    #[test]
    fn icon_ids_match_the_host_lucide_map() {
        let ids: BTreeSet<UiIconId> = UI_ICON_IDS.iter().copied().collect();
        let mapped: BTreeSet<UiIconId> = HOST_LUCIDE_MAP.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, mapped);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::UI_ICON_IDS.len(), 34);
        let _: Option<crate::UiIconId> = None;
    }
}
