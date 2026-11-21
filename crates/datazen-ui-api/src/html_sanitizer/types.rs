pub const STRIP_TAGS: [&str; 17] = [
    "script", "iframe", "object", "embed", "form", "input", "textarea", "select", "button",
    "style", "link", "meta", "base", "applet", "noscript", "svg", "math",
];

pub const VOID_TAGS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HtmlNode {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<HtmlNode>,
    },
    Text(String),
    Comment(String),
}

pub fn is_strip_tag(tag: &str) -> bool {
    STRIP_TAGS.iter().any(|candidate| tag.eq_ignore_ascii_case(candidate))
}

pub fn is_void_tag(tag: &str) -> bool {
    VOID_TAGS.iter().any(|candidate| tag.eq_ignore_ascii_case(candidate))
}

pub fn is_event_attr(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 3
        && (bytes[0] == b'o' || bytes[0] == b'O')
        && (bytes[1] == b'n' || bytes[1] == b'N')
        && bytes[2].is_ascii_alphabetic()
}

pub fn is_javascript_href(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.len() >= "javascript:".len()
        && trimmed[.."javascript:".len()].eq_ignore_ascii_case("javascript:")
}

pub fn decode_entities(value: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0;
    while cursor < value.len() {
        let Some(relative) = value[cursor..].find('&') else {
            out.push_str(&value[cursor..]);
            break;
        };
        let start = cursor + relative;
        out.push_str(&value[cursor..start]);
        let Some(relative_end) = value[start..].find(';') else {
            out.push('&');
            cursor = start + 1;
            continue;
        };
        let end = start + relative_end;
        let entity = &value[start + 1..end];
        let decoded = if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(decimal) = entity.strip_prefix('#') {
            decimal.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "colon" => Some(':'),
                _ => None,
            }
        };
        match decoded {
            Some(character) => out.push(character),
            None => out.push_str(&value[start..=end]),
        }
        cursor = end + 1;
    }
    out
}

pub fn get_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub fn set_attr(attrs: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some((_, existing)) = attrs.iter_mut().find(|(key, _)| key.eq_ignore_ascii_case(name)) {
        *existing = value.to_owned();
        return;
    }
    attrs.push((name.to_owned(), value.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seventeen_strip_tags_are_case_insensitive() {
        assert_eq!(STRIP_TAGS.len(), 17);
        for tag in STRIP_TAGS {
            assert!(is_strip_tag(tag));
            assert!(is_strip_tag(&tag.to_uppercase()));
        }
        assert!(!is_strip_tag("div"));
        assert!(!is_strip_tag("scripter"));
    }

    #[test]
    fn event_attributes_need_on_plus_one_letter() {
        for name in ["onclick", "ONERROR", "onMouseOver", "on1"] {
            assert_eq!(is_event_attr(name), name != "on1", "{name}");
        }
        assert!(!is_event_attr("on"));
        assert!(!is_event_attr("data-onclick"));
    }

    #[test]
    fn javascript_hrefs_skip_leading_whitespace_and_ignore_case() {
        assert!(is_javascript_href("javascript:alert(1)"));
        assert!(is_javascript_href("  JaVaScRiPt:alert(1)"));
        assert!(!is_javascript_href("https://example.com"));
        assert!(!is_javascript_href(" java script:alert(1)"));
    }

    #[test]
    fn numeric_and_named_entities_decode_in_attribute_values() {
        assert_eq!(decode_entities("java&#115;cript&#58;alert(1)"), "javascript:alert(1)");
        assert_eq!(decode_entities("java&#x73;cript&#x3A;alert(1)"), "javascript:alert(1)");
        assert_eq!(decode_entities("&amp;&lt;&gt;&quot;&apos;&colon;"), "&<>\"':");
        assert_eq!(decode_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn attribute_helpers_overwrite_without_duplicating() {
        let mut attrs = vec![("href".to_owned(), "old".to_owned())];
        set_attr(&mut attrs, "HREF", "#");
        assert_eq!(attrs.len(), 1);
        assert_eq!(get_attr(&attrs, "href"), Some("#"));
        set_attr(&mut attrs, "target", "_blank");
        assert_eq!(get_attr(&attrs, "target"), Some("_blank"));
    }
}
