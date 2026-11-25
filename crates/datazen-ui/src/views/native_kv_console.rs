//! Local command and exact-key completions for the native Redis console.
use super::{control, AppState, KvWorkbench};
use datazen_ui_api::native_kv::{KvKey, KvTool, REDIS_CONSOLE_READ_COMMANDS};
use rune::{col, text, tok::Theme, El};

pub(super) struct Completion {
    pub id: String,
    pub label: String,
    pub text: String,
}

pub(super) fn completions(text: &str, keys: &[KvKey]) -> Vec<Completion> {
    let text = text.trim_start();
    if text.is_empty() || text.chars().any(char::is_control) {
        return Vec::new();
    }
    if let Some((verb, argument)) = text.split_once(' ') {
        let primary = verb.to_ascii_uppercase();
        let key_prefix = argument.trim_start();
        if key_prefix.contains([' ', '"', '\\'])
            || !matches!(
                primary.as_str(),
                "GET"
                    | "TYPE"
                    | "TTL"
                    | "PTTL"
                    | "EXISTS"
                    | "HLEN"
                    | "HGET"
                    | "STRLEN"
                    | "LLEN"
                    | "LINDEX"
                    | "SCARD"
                    | "SISMEMBER"
                    | "ZCARD"
                    | "ZSCORE"
                    | "XLEN"
            )
        {
            return Vec::new();
        }
        let mut found: Vec<_> = keys
            .iter()
            .map(|key| key.key.as_str())
            .filter(|key| {
                key.starts_with(key_prefix)
                    && key != &key_prefix
                    && key.len() <= 128
                    && !key.chars().any(char::is_control)
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        return found
            .into_iter()
            .take(5)
            .enumerate()
            .map(|(index, key)| {
                let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
                Completion {
                    id: format!("native-kv-complete-key-{index}"),
                    label: key.to_owned(),
                    text: format!("{primary} \"{escaped}\" "),
                }
            })
            .collect();
    }
    let prefix = text.to_ascii_uppercase();
    REDIS_CONSOLE_READ_COMMANDS
        .iter()
        .copied()
        .filter(|verb| verb.starts_with(&prefix) && *verb != prefix)
        .take(5)
        .map(|verb| Completion {
            id: format!("native-kv-complete-{verb}"),
            label: verb.to_owned(),
            text: format!("{verb} "),
        })
        .collect()
}

/// Recognize only an unambiguous, simple SET key value. The reviewed action
/// uses the native create-only command, never forwards a raw write.
pub(super) fn simple_set(text: &str) -> Option<(String, String)> {
    if text.chars().any(char::is_control) || text.contains(['"', '\\']) {
        return None;
    }
    let mut parts = text.split_ascii_whitespace();
    if !parts.next()?.eq_ignore_ascii_case("SET") {
        return None;
    }
    let (key, value) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && key.len() <= 1024 && value.len() <= 4096)
        .then(|| (key.to_owned(), value.to_owned()))
}

pub(super) fn set_review(state: &KvWorkbench, theme: Theme) -> El {
    let Some((key, value)) = &state.tools.console_set_review else {
        return col();
    };
    if simple_set(&state.tools.command.text).as_ref() != Some(&(key.clone(), value.clone())) {
        return col();
    }
    let key_label = format!(
        "Create {key:?} on DB {} without overwriting?",
        state.db_index.unwrap_or(0)
    );
    col()
        .gap(4.0)
        .child(text(key_label).key("native-kv-console-set-review").wrap())
        .child(
            control("native-kv-console-confirm-set", "Create new String", theme).on_press(
                |s: &mut AppState| {
                    let Some(vm) = s.native_kv.as_mut() else {
                        return;
                    };
                    if let Some((key, value)) = vm.tools.console_set_review.take() {
                        if vm.writable
                            && simple_set(&vm.tools.command.text).as_ref()
                                == Some(&(key.clone(), value.clone()))
                        {
                            s.kv_tool(KvTool::CreateString { key, value });
                        }
                    }
                },
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_set_only_accepts_three_exact_tokens() {
        assert_eq!(
            simple_set("SET key value"),
            Some(("key".into(), "value".into()))
        );
        assert!(simple_set("SET key value XX").is_none());
        assert!(simple_set("SET key value\nGET key").is_none());
        assert!(simple_set("SET key \"multi word\"").is_none());
    }

    #[test]
    fn completion_is_key_scoped_and_escapes_literal_bytes() {
        let keys = [KvKey {
            key: "with \" space".into(),
            key_type: "string".into(),
            ttl: -1,
        }];
        assert_eq!(completions("PIN", &keys)[0].text, "PING ");
        assert_eq!(
            completions("GET with", &keys)[0].text,
            "GET \"with \\\" space\" "
        );
        assert!(completions("INFO with", &keys).is_empty());
        assert!(completions("SET with", &keys).is_empty());
    }
}
