pub const DEFAULT_PANE_ID: &str = "main";
pub const PANE_SEPARATOR: &str = "::";

pub fn pane_key(panel_id: &str, pane_id: Option<&str>) -> String {
    let pane_id = pane_id.unwrap_or(DEFAULT_PANE_ID);
    if pane_id == DEFAULT_PANE_ID {
        panel_id.to_owned()
    } else {
        format!("{panel_id}{PANE_SEPARATOR}{pane_id}")
    }
}

pub fn panel_id_of_pane_key(key: &str) -> String {
    match key.find(PANE_SEPARATOR) {
        Some(index) => key[..index].to_owned(),
        None => key.to_owned(),
    }
}

pub fn pane_id_of_pane_key(key: &str) -> String {
    match key.find(PANE_SEPARATOR) {
        Some(index) => key[index + PANE_SEPARATOR.len()..].to_owned(),
        None => DEFAULT_PANE_ID.to_owned(),
    }
}

pub fn is_pane_key_of_panel(key: &str, panel_id: &str) -> bool {
    panel_id_of_pane_key(key) == panel_id
}

pub fn pane_keys_of_panel(keys: &[String], panel_id: &str) -> Vec<String> {
    keys.iter()
        .filter(|key| is_pane_key_of_panel(key, panel_id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pane_uses_the_bare_panel_id() {
        assert_eq!(pane_key("p1", None), "p1");
        assert_eq!(pane_key("p1", Some("main")), "p1");
        assert_eq!(pane_key("p1", Some("left")), "p1::left");
    }

    #[test]
    fn keys_split_back_into_panel_and_pane() {
        assert_eq!(panel_id_of_pane_key("p1"), "p1");
        assert_eq!(pane_id_of_pane_key("p1"), DEFAULT_PANE_ID);
        assert_eq!(panel_id_of_pane_key("p1::left"), "p1");
        assert_eq!(pane_id_of_pane_key("p1::left"), "left");
    }

    #[test]
    fn panel_keys_are_filtered_in_input_order() {
        let keys = [
            "p1".to_owned(),
            "p2".to_owned(),
            "p1::left".to_owned(),
            "p12".to_owned(),
        ];
        assert_eq!(pane_keys_of_panel(&keys, "p1"), ["p1", "p1::left"]);
        assert!(is_pane_key_of_panel("p12", "p1") == false);
        assert!(is_pane_key_of_panel("p1::left", "p1"));
    }
}
