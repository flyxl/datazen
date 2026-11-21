#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvAiFacts {
    pub connection_name: String,
    pub db_session_id: String,
    pub database: Option<String>,
    pub selected_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvAiContext {
    pub key_name: String,
    pub block: String,
}

fn selected_key_name_of(selected_key: Option<&str>) -> Option<&str> {
    match selected_key {
        Some(key) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

pub fn has_kv_ai_facts(selected_key: Option<&str>) -> bool {
    selected_key_name_of(selected_key).is_some()
}

pub fn build_kv_ai_context(facts: &KvAiFacts) -> Option<KvAiContext> {
    let key_name = selected_key_name_of(facts.selected_key.as_deref())?;
    let block = [
        "[kv-context]",
        &format!("connection={}", facts.connection_name),
        &format!("session={}", facts.db_session_id),
        &format!("database={}", facts.database.as_deref().unwrap_or("")),
        &format!("selected_key={key_name}"),
        "[/kv-context]",
    ]
    .join("\n");
    Some(KvAiContext { key_name: key_name.to_owned(), block })
}

pub fn compose_kv_ai_message(content: &str, context: Option<&KvAiContext>) -> String {
    match context {
        Some(context) => format!("{content}\n\n{}", context.block),
        None => content.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(selected_key: Option<&str>) -> KvAiFacts {
        KvAiFacts {
            connection_name: "Local Redis".to_owned(),
            db_session_id: "s1".to_owned(),
            database: Some("db0".to_owned()),
            selected_key: selected_key.map(str::to_owned),
        }
    }

    #[test]
    fn facts_require_a_nonblank_selected_key() {
        assert!(!has_kv_ai_facts(None));
        assert!(!has_kv_ai_facts(Some("")));
        assert!(!has_kv_ai_facts(Some("   ")));
        assert!(has_kv_ai_facts(Some("user:1")));
    }

    #[test]
    fn context_is_null_without_a_key_and_keeps_the_untrimmed_name() {
        assert!(build_kv_ai_context(&facts(None)).is_none());
        assert!(build_kv_ai_context(&facts(Some("  "))).is_none());
        let context = build_kv_ai_context(&facts(Some("  spaced:key  "))).expect("context");
        assert_eq!(context.key_name, "  spaced:key  ");
        assert!(context.block.contains("selected_key=  spaced:key  "));
    }

    #[test]
    fn block_is_a_stable_fact_list() {
        let context = build_kv_ai_context(&facts(Some("user:1"))).expect("context");
        assert_eq!(
            context.block,
            "[kv-context]\nconnection=Local Redis\nsession=s1\ndatabase=db0\nselected_key=user:1\n[/kv-context]"
        );
        let mut without_db = facts(Some("user:1"));
        without_db.database = None;
        let context = build_kv_ai_context(&without_db).expect("context");
        assert!(context.block.contains("database=\n"));
    }

    #[test]
    fn block_exposes_no_credentials_or_value_digest() {
        let mut input = facts(Some("secret-key"));
        input.connection_name = "redis://user:password@host:6379".to_owned();
        let context = build_kv_ai_context(&input).expect("context");
        assert!(context.block.contains("redis://user:password@host:6379"), "the host-owned label is data");
        assert!(!context.block.contains("ttl="));
        assert!(!context.block.contains("value="));
        assert!(!context.block.contains("size="));
    }

    #[test]
    fn message_composition_appends_only_when_context_exists() {
        let context = build_kv_ai_context(&facts(Some("user:1")));
        assert_eq!(compose_kv_ai_message("why?", context.as_ref()), format!("why?\n\n{}", context.expect("context").block));
        assert_eq!(compose_kv_ai_message("why?", None), "why?");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::has_kv_ai_facts;
        let _ = crate::build_kv_ai_context;
        let _ = crate::compose_kv_ai_message;
        let _: Option<crate::KvAiFacts> = None;
        let _: Option<crate::KvAiContext> = None;
    }
}
