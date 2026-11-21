use crate::html_sanitizer::parse::{parse_html, serialize_html};
use crate::html_sanitizer::types::{
    HtmlNode, STRIP_TAGS, is_event_attr, is_javascript_href, is_strip_tag, set_attr,
};

fn sanitize_nodes(nodes: Vec<HtmlNode>) -> Vec<HtmlNode> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            HtmlNode::Text(_) | HtmlNode::Comment(_) => Some(node),
            HtmlNode::Element { tag, attrs, children } => {
                if is_strip_tag(&tag) {
                    return None;
                }
                let mut attrs: Vec<(String, String)> =
                    attrs.into_iter().filter(|(name, _)| !is_event_attr(name)).collect();
                if tag.eq_ignore_ascii_case("a") {
                    if let Some((_, href)) = attrs.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case("href")) {
                        if is_javascript_href(href) {
                            *href = "#".to_owned();
                        }
                    }
                    set_attr(&mut attrs, "target", "_blank");
                    set_attr(&mut attrs, "rel", "noopener noreferrer");
                }
                Some(HtmlNode::Element { tag, attrs, children: sanitize_nodes(children) })
            }
        })
        .collect()
}

pub fn sanitize_html(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }
    serialize_html(&sanitize_nodes(parse_html(html)))
}

fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..]
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
        .map(|offset| from + offset)
}

fn tag_open_at(input: &str, index: usize, tag: &str) -> bool {
    if input.as_bytes().get(index) != Some(&b'<') {
        return false;
    }
    let after = index + 1;
    if after + tag.len() > input.len() || !input[after..after + tag.len()].eq_ignore_ascii_case(tag) {
        return false;
    }
    matches!(input.as_bytes().get(after + tag.len()), Some(b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'/' | b'>'))
}

fn tag_end(input: &str, open: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = open + 1;
    let mut quote = None;
    while index < bytes.len() {
        match quote {
            Some(character) if bytes[index] == character => quote = None,
            Some(_) => {}
            None if bytes[index] == b'"' || bytes[index] == b'\'' => quote = Some(bytes[index]),
            None if bytes[index] == b'>' => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

fn remove_paired_tag(input: &str, tag: &str) -> String {
    let mut result = input.to_owned();
    let mut cursor = 0;
    let open_marker = format!("<{tag}");
    let close_marker = format!("</{tag}>");
    loop {
        let Some(open) = find_ci(&result, &open_marker, cursor) else {
            break;
        };
        if !tag_open_at(&result, open, tag) {
            cursor = open + 1;
            continue;
        }
        let Some(close) = find_ci(&result, &close_marker, open + 1) else {
            cursor = open + 1;
            continue;
        };
        result.replace_range(open..close + close_marker.len(), "");
        cursor = open;
    }
    result
}

fn remove_self_closing(input: &str, tag: &str) -> String {
    let mut result = input.to_owned();
    let mut cursor = 0;
    let open_marker = format!("<{tag}");
    loop {
        let Some(open) = find_ci(&result, &open_marker, cursor) else {
            break;
        };
        if !tag_open_at(&result, open, tag) {
            cursor = open + 1;
            continue;
        }
        let Some(end) = tag_end(&result, open) else {
            cursor = open + 1;
            continue;
        };
        result.replace_range(open..end + 1, "");
        cursor = open;
    }
    result
}

fn remove_event_attributes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace()
            && matches!(bytes.get(index + 1), Some(b'o' | b'O'))
            && matches!(bytes.get(index + 2), Some(b'n' | b'N'))
            && bytes.get(index + 3).is_some_and(u8::is_ascii_lowercase)
        {
            let mut cursor = index + 3;
            while cursor < bytes.len() && (bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'-') {
                cursor += 1;
            }
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'=') {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if matches!(bytes.get(cursor), Some(b'"' | b'\'')) {
                    let quote = bytes[cursor];
                    cursor += 1;
                    while cursor < bytes.len() && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    if cursor < bytes.len() {
                        cursor += 1;
                    }
                    index = cursor;
                    continue;
                }
            }
        }
        let character = input[index..].chars().next().unwrap_or('\0');
        out.push(character);
        index += character.len_utf8();
    }
    out
}

fn href_match_end(input: &str, href: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = href + 4;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if bytes.get(index) != Some(&b'=') {
        return None;
    }
    index += 1;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let mut had_quote = false;
    if matches!(bytes.get(index), Some(b'"' | b'\'')) {
        had_quote = true;
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
    }
    let scheme = "javascript:";
    if index + scheme.len() > bytes.len() || !input[index..index + scheme.len()].eq_ignore_ascii_case(scheme) {
        return None;
    }
    let mut end = index + scheme.len();
    while end < bytes.len() && !matches!(bytes[end], b'"' | b'\'') {
        end += 1;
    }
    if had_quote && end < bytes.len() {
        end += 1;
    }
    Some(end)
}

fn neuter_href(input: &str) -> String {
    let mut result = input.to_owned();
    let mut cursor = 0;
    loop {
        let Some(href) = find_ci(&result, "href", cursor) else {
            break;
        };
        let Some(end) = href_match_end(&result, href) else {
            cursor = href + 4;
            continue;
        };
        result.replace_range(href..end, "href=\"#\"");
        cursor = href + 9;
    }
    result
}

pub fn regex_sanitize(html: &str) -> String {
    let mut result = html.to_owned();
    for tag in STRIP_TAGS {
        result = remove_paired_tag(&result, tag);
        result = remove_self_closing(&result, tag);
    }
    result = remove_event_attributes(&result);
    neuter_href(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(sanitize_html(""), "");
        assert_eq!(regex_sanitize(""), "");
    }

    #[test]
    fn harmless_markup_survives_with_normalized_attributes() {
        let out = sanitize_html("<p class=\"x\">hello <b>world</b></p>");
        assert_eq!(out, "<p class=\"x\">hello <b>world</b></p>");
    }

    #[test]
    fn strip_tags_remove_the_element_and_its_content() {
        for tag in ["script", "iframe", "svg", "style", "form", "object", "math"] {
            let source = format!("<div>before<{tag}>alert(1)</{tag}>after</div>");
            assert_eq!(sanitize_html(&source), "<div>beforeafter</div>", "{tag}");
        }
    }

    #[test]
    fn strip_tags_remove_self_closing_forms() {
        assert_eq!(sanitize_html("<div>a<input value=\"x\">b</div>"), "<div>ab</div>");
        assert_eq!(sanitize_html("x<img src=\"a\" onerror=\"alert(1)\">y"), "x<img src=\"a\">y");
    }

    #[test]
    fn event_handlers_are_removed_from_every_element() {
        assert_eq!(
            sanitize_html("<div onclick=\"x\" ONERROR='y' data-keep=\"z\">t</div>"),
            "<div data-keep=\"z\">t</div>"
        );
    }

    #[test]
    fn javascript_links_are_neutered_and_anchors_are_hardened() {
        assert_eq!(
            sanitize_html("<a href=\"javascript:alert(1)\">x</a>"),
            "<a href=\"#\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        );
        assert_eq!(
            sanitize_html("<a href=' JaVaScRiPt:alert(1)' target=\"_self\" rel=\"x\">x</a>"),
            "<a href=\"#\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn entity_encoded_javascript_schemes_are_decoded_then_neutered() {
        assert_eq!(
            sanitize_html("<a href='java&#115;cript&#58;alert(1)'>x</a>"),
            "<a href=\"#\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn safe_links_keep_their_href_and_still_get_target_and_rel() {
        assert_eq!(
            sanitize_html("<a href=\"https://example.com\">x</a>"),
            "<a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn nested_dangerous_tags_are_removed_recursively() {
        assert_eq!(
            sanitize_html("<div><p>a<script>b</script>c</p><iframe>d</iframe></div>"),
            "<div><p>ac</p></div>"
        );
    }

    #[test]
    fn comments_are_preserved() {
        assert_eq!(sanitize_html("<!-- keep --><p>x</p>"), "<!-- keep --><p>x</p>");
    }

    #[test]
    fn regex_fallback_removes_pairs_events_and_javascript_hrefs() {
        assert_eq!(regex_sanitize("<div>a<script>alert(1)</script>b</div>"), "<div>ab</div>");
        assert_eq!(regex_sanitize("<p onclick=\"x\" data-keep=\"1\">t</p>"), "<p data-keep=\"1\">t</p>");
        assert_eq!(regex_sanitize("<a href=\"javascript:alert(1)\">x</a>"), "<a href=\"#\">x</a>");
    }

    #[test]
    fn regex_fallback_handles_case_and_single_quotes() {
        assert_eq!(regex_sanitize("<IFRAME SRC='x'>bad</IFRAME>ok"), "ok");
        assert_eq!(regex_sanitize("<a HREF='javascript:alert(1)'>x</a>"), "<a href=\"#\">x</a>");
    }
}
