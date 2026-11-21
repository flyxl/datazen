use std::collections::HashMap;

use serde_json::Value;

use super::ids::{join_relative_path, ICON_EXTENSIONS};

pub fn parse_charts_json(bytes: &[u8]) -> Option<HashMap<String, Vec<String>>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let json: Value = serde_json::from_str(text).ok()?;
    let object = json.as_object()?;
    let mut result = HashMap::new();
    for (key, value) in object {
        if let Some(items) = value.as_array() {
            if !items.is_empty() && items.iter().all(|item| item.is_string()) {
                let colors: Vec<String> = items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect();
                result.insert(key.clone(), colors);
            }
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn probe_icon<F>(read_file: F, icons_dir: &str, semantic_id: &str) -> Option<(String, Vec<u8>)>
where
    F: Fn(&str) -> Option<Vec<u8>>,
{
    for extension in ICON_EXTENSIONS.iter() {
        let reference = format!("{semantic_id}{extension}");
        let relative_path = join_relative_path(icons_dir, &reference);
        if let Some(bytes) = read_file(&relative_path) {
            if !bytes.is_empty() {
                return Some((relative_path, bytes));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_json_keeps_only_nonempty_string_arrays() {
        let bytes = br##"{"series":["#f00","#0f0"],"bad":[1,2],"empty":[],"other":"x"}"##;
        let parsed = parse_charts_json(bytes).unwrap();
        assert_eq!(parsed["series"], ["#f00", "#0f0"]);
        assert!(!parsed.contains_key("bad"));
        assert!(!parsed.contains_key("empty"));
        assert!(!parsed.contains_key("other"));
    }

    #[test]
    fn charts_json_rejects_empty_and_malformed_payloads() {
        assert_eq!(parse_charts_json(br#"{}"#), None);
        assert_eq!(parse_charts_json(br#"[]"#), None);
        assert_eq!(parse_charts_json(b"not json"), None);
        assert_eq!(parse_charts_json(&[0xff, 0xfe]), None);
    }

    #[test]
    fn icon_probing_tries_extensions_in_order_and_skips_empty_files() {
        let result = probe_icon(
            |path| {
                if path == "icons/logo.webp" {
                    Some(Vec::new())
                } else if path == "icons/logo.png" {
                    Some(vec![1, 2, 3])
                } else {
                    None
                }
            },
            "icons",
            "logo",
        );
        assert_eq!(result, Some(("icons/logo.png".to_owned(), vec![1, 2, 3])));

        let none = probe_icon(|_| None, "icons", "logo");
        assert_eq!(none, None);
    }
}
