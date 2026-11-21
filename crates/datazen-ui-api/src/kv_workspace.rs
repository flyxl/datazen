#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvSlotName {
    ContextBar,
    StatusBar,
    KeyPropsSidebar,
    ConnectionHome,
}

impl KvSlotName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContextBar => "contextBar",
            Self::StatusBar => "statusBar",
            Self::KeyPropsSidebar => "keyPropsSidebar",
            Self::ConnectionHome => "connectionHome",
        }
    }
}

pub const KV_SLOT_NAMES: [KvSlotName; 4] = [
    KvSlotName::ContextBar,
    KvSlotName::StatusBar,
    KvSlotName::KeyPropsSidebar,
    KvSlotName::ConnectionHome,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KvWorkspaceCapabilities {
    pub context_bar: bool,
    pub status_bar: bool,
    pub key_props_sidebar: bool,
    pub home: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KvDriverMeta {
    pub kv_workspace: Option<KvWorkspaceCapabilities>,
}

pub fn capability_key_for_slot(slot: KvSlotName) -> &'static str {
    match slot {
        KvSlotName::ConnectionHome => "home",
        KvSlotName::ContextBar => "contextBar",
        KvSlotName::StatusBar => "statusBar",
        KvSlotName::KeyPropsSidebar => "keyPropsSidebar",
    }
}

pub fn has_kv_slot_capability(meta: Option<&KvDriverMeta>, slot: KvSlotName) -> bool {
    let Some(capabilities) = meta.and_then(|entry| entry.kv_workspace) else {
        return false;
    };
    match slot {
        KvSlotName::ContextBar => capabilities.context_bar,
        KvSlotName::StatusBar => capabilities.status_bar,
        KvSlotName::KeyPropsSidebar => capabilities.key_props_sidebar,
        KvSlotName::ConnectionHome => capabilities.home,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvSlotComponent {
    pub database_type: String,
    pub slot: KvSlotName,
}

pub type KvSlotMetaProvider = dyn Fn(&str) -> Option<KvDriverMeta>;
pub type KvSlotComponentProvider = dyn Fn(&str, KvSlotName) -> Option<KvSlotComponent>;

pub fn get_kv_slot_component(
    database_type: Option<&str>,
    slot: KvSlotName,
    meta_provider: &KvSlotMetaProvider,
    component_provider: &KvSlotComponentProvider,
) -> Option<KvSlotComponent> {
    let database_type = database_type?;
    let meta = meta_provider(database_type);
    if !has_kv_slot_capability(meta.as_ref(), slot) {
        return None;
    }
    component_provider(database_type, slot)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn capabilities(
        context: bool,
        status: bool,
        props: bool,
        home: bool,
    ) -> KvWorkspaceCapabilities {
        KvWorkspaceCapabilities {
            context_bar: context,
            status_bar: status,
            key_props_sidebar: props,
            home,
        }
    }

    #[test]
    fn slot_names_and_literals_are_pinned() {
        assert_eq!(KV_SLOT_NAMES.len(), 4);
        assert_eq!(KV_SLOT_NAMES[0], KvSlotName::ContextBar);
        assert_eq!(KV_SLOT_NAMES[3], KvSlotName::ConnectionHome);
        let literals: Vec<&str> = KV_SLOT_NAMES.iter().map(|slot| slot.as_str()).collect();
        assert_eq!(
            literals,
            [
                "contextBar",
                "statusBar",
                "keyPropsSidebar",
                "connectionHome"
            ]
        );
        assert_eq!(capability_key_for_slot(KvSlotName::ConnectionHome), "home");
        assert_eq!(
            capability_key_for_slot(KvSlotName::ContextBar),
            "contextBar"
        );
        assert_eq!(
            KvWorkspaceCapabilities::default(),
            capabilities(false, false, false, false)
        );
    }

    #[test]
    fn missing_meta_or_capability_object_answer_false() {
        assert!(!has_kv_slot_capability(None, KvSlotName::ContextBar));
        let no_caps = KvDriverMeta::default();
        for slot in KV_SLOT_NAMES {
            assert!(!has_kv_slot_capability(Some(&no_caps), slot));
        }
    }

    #[test]
    fn each_slot_reads_only_its_own_flag() {
        let only_context = KvDriverMeta {
            kv_workspace: Some(capabilities(true, false, false, false)),
        };
        assert!(has_kv_slot_capability(
            Some(&only_context),
            KvSlotName::ContextBar
        ));
        assert!(!has_kv_slot_capability(
            Some(&only_context),
            KvSlotName::StatusBar
        ));
        assert!(!has_kv_slot_capability(
            Some(&only_context),
            KvSlotName::ConnectionHome
        ));

        let only_home = KvDriverMeta {
            kv_workspace: Some(capabilities(false, false, false, true)),
        };
        assert!(has_kv_slot_capability(
            Some(&only_home),
            KvSlotName::ConnectionHome
        ));
        assert!(!has_kv_slot_capability(
            Some(&only_home),
            KvSlotName::ContextBar
        ));

        let all = KvDriverMeta {
            kv_workspace: Some(capabilities(true, true, true, true)),
        };
        for slot in KV_SLOT_NAMES {
            assert!(has_kv_slot_capability(Some(&all), slot));
        }
    }

    #[test]
    fn slot_resolution_requires_both_gates_and_short_circuits() {
        let meta_calls = Rc::new(RefCell::new(Vec::new()));
        let component_calls = Rc::new(RefCell::new(Vec::new()));
        let meta_seen = meta_calls.clone();
        let component_seen = component_calls.clone();
        let meta_provider = move |database_type: &str| {
            meta_seen.borrow_mut().push(database_type.to_owned());
            if database_type == "redis" {
                Some(KvDriverMeta {
                    kv_workspace: Some(capabilities(true, false, false, false)),
                })
            } else {
                None
            }
        };
        let component_provider = move |database_type: &str, slot: KvSlotName| {
            component_seen
                .borrow_mut()
                .push((database_type.to_owned(), slot));
            Some(KvSlotComponent {
                database_type: database_type.to_owned(),
                slot,
            })
        };

        assert_eq!(
            get_kv_slot_component(
                None,
                KvSlotName::ContextBar,
                &meta_provider,
                &component_provider
            ),
            None
        );
        assert!(meta_calls.borrow().is_empty());
        assert!(component_calls.borrow().is_empty());

        assert_eq!(
            get_kv_slot_component(
                Some("mysql"),
                KvSlotName::ContextBar,
                &meta_provider,
                &component_provider
            ),
            None
        );
        assert_eq!(component_calls.borrow().len(), 0);

        assert_eq!(
            get_kv_slot_component(
                Some("redis"),
                KvSlotName::StatusBar,
                &meta_provider,
                &component_provider
            ),
            None
        );
        assert_eq!(component_calls.borrow().len(), 0);

        assert_eq!(
            get_kv_slot_component(
                Some("redis"),
                KvSlotName::ContextBar,
                &meta_provider,
                &component_provider
            ),
            Some(KvSlotComponent {
                database_type: "redis".to_owned(),
                slot: KvSlotName::ContextBar,
            })
        );
        assert_eq!(component_calls.borrow().len(), 1);
    }
}
