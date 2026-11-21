use crate::html_sanitizer::types::{decode_entities, is_void_tag, HtmlNode};

fn escape_text(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}

fn skip_wsp(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && (bytes[index] as char).is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn parse_attributes(input: &str, mut index: usize) -> (Vec<(String, String)>, usize, bool) {
    let bytes = input.as_bytes();
    let mut attrs = Vec::new();
    loop {
        index = skip_wsp(bytes, index);
        if index >= bytes.len() {
            return (attrs, index, false);
        }
        if bytes[index] == b'>' {
            return (attrs, index + 1, false);
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'>') {
            return (attrs, index + 2, true);
        }
        if bytes[index] == b'/' {
            index += 1;
            continue;
        }
        let name_start = index;
        while index < bytes.len()
            && !matches!(bytes[index], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'=' | b'/' | b'>')
        {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name = input[name_start..index].to_ascii_lowercase();
        index = skip_wsp(bytes, index);
        let mut value = String::new();
        if bytes.get(index) == Some(&b'=') {
            index = skip_wsp(bytes, index + 1);
            match bytes.get(index).copied() {
                Some(b'"') | Some(b'\'') => {
                    let quote = bytes[index];
                    index += 1;
                    let value_start = index;
                    while index < bytes.len() && bytes[index] != quote {
                        index += 1;
                    }
                    value = decode_entities(&input[value_start..index]);
                    if index < bytes.len() {
                        index += 1;
                    }
                }
                _ => {
                    let value_start = index;
                    while index < bytes.len()
                        && !matches!(bytes[index], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'>')
                    {
                        index += 1;
                    }
                    value = decode_entities(&input[value_start..index]);
                }
            }
        }
        attrs.push((name, value));
    }
}

fn is_tag_start(bytes: &[u8], index: usize) -> bool {
    index + 1 < bytes.len() && bytes[index] == b'<' && bytes[index + 1].is_ascii_alphabetic()
}

fn parse_nodes(input: &str, position: &mut usize, stop_tag: Option<&str>) -> Vec<HtmlNode> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut flush = |nodes: &mut Vec<HtmlNode>, text: &mut String| {
        if !text.is_empty() {
            nodes.push(HtmlNode::Text(decode_entities(text)));
            text.clear();
        }
    };

    while *position < bytes.len() {
        if input[*position..].starts_with("<!--") {
            flush(&mut out, &mut text);
            let start = *position + 4;
            if let Some(relative) = input[start..].find("-->") {
                let end = start + relative;
                out.push(HtmlNode::Comment(input[start..end].to_owned()));
                *position = end + 3;
            } else {
                out.push(HtmlNode::Comment(input[start..].to_owned()));
                *position = bytes.len();
            }
            continue;
        }
        if input[*position..].starts_with("</") {
            let start = *position + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'>' {
                end += 1;
            }
            let name = input[start..end].trim().to_ascii_lowercase();
            *position = (end + 1).min(bytes.len());
            flush(&mut out, &mut text);
            if stop_tag.is_some_and(|stop| name == stop) {
                return out;
            }
            continue;
        }
        if is_tag_start(bytes, *position) {
            flush(&mut out, &mut text);
            let start = *position + 1;
            let mut end = start;
            while end < bytes.len()
                && !matches!(bytes[end], b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'/' | b'>')
            {
                end += 1;
            }
            if start == end {
                text.push('<');
                *position += 1;
                continue;
            }
            let tag = input[start..end].to_ascii_lowercase();
            let (attrs, next, self_closing) = parse_attributes(input, end);
            *position = next.min(bytes.len());
            let children = if self_closing || is_void_tag(&tag) {
                Vec::new()
            } else {
                parse_nodes(input, position, Some(&tag))
            };
            out.push(HtmlNode::Element { tag, attrs, children });
            continue;
        }
        let character = input[*position..].chars().next().unwrap_or('\0');
        text.push(character);
        *position += character.len_utf8();
    }
    flush(&mut out, &mut text);
    out
}

pub fn parse_html(input: &str) -> Vec<HtmlNode> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut position = 0;
    parse_nodes(input, &mut position, None)
}

pub fn serialize_html(nodes: &[HtmlNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(value) => out.push_str(&escape_text(value)),
            HtmlNode::Comment(value) => {
                out.push_str("<!--");
                out.push_str(value);
                out.push_str("-->");
            }
            HtmlNode::Element { tag, attrs, children } => {
                out.push('<');
                out.push_str(tag);
                for (name, value) in attrs {
                    out.push(' ');
                    out.push_str(name);
                    out.push_str("=\"");
                    out.push_str(&escape_attr(value));
                    out.push('"');
                }
                out.push('>');
                if !is_void_tag(tag) {
                    out.push_str(&serialize_html(children));
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html_sanitizer::types::HtmlNode;

    #[test]
    fn parses_nested_elements_with_attributes() {
        let nodes = parse_html("<div class='x'><b>hi</b></div>");
        assert_eq!(
            nodes,
            vec![HtmlNode::Element {
                tag: "div".to_owned(),
                attrs: vec![("class".to_owned(), "x".to_owned())],
                children: vec![HtmlNode::Element {
                    tag: "b".to_owned(),
                    attrs: Vec::new(),
                    children: vec![HtmlNode::Text("hi".to_owned())],
                }],
            }]
        );
    }

    #[test]
    fn void_and_self_closing_tags_have_no_children() {
        let nodes = parse_html("<br><img src='a'/><custom/>");
        assert_eq!(nodes.len(), 3);
        assert!(matches!(&nodes[0], HtmlNode::Element { tag, children, .. } if tag == "br" && children.is_empty()));
        assert!(matches!(&nodes[1], HtmlNode::Element { tag, .. } if tag == "img"));
        assert!(matches!(&nodes[2], HtmlNode::Element { tag, children, .. } if tag == "custom" && children.is_empty()));
    }

    #[test]
    fn comments_and_entities_are_preserved() {
        let nodes = parse_html("<!-- c -->&lt;a&amp;b&gt;");
        assert_eq!(nodes[0], HtmlNode::Comment(" c ".to_owned()));
        assert_eq!(nodes[1], HtmlNode::Text("<a&b>".to_owned()));
    }

    #[test]
    fn entity_attribute_values_are_decoded_for_the_sanitizer() {
        let nodes = parse_html("<a href='java&#115;cript:alert(1)'>x</a>");
        assert_eq!(
            nodes,
            vec![HtmlNode::Element {
                tag: "a".to_owned(),
                attrs: vec![("href".to_owned(), "javascript:alert(1)".to_owned())],
                children: vec![HtmlNode::Text("x".to_owned())],
            }]
        );
    }

    #[test]
    fn serialization_round_trips_the_parsed_tree() {
        let source = "<div class=\"a\"><b>hi</b><br></div><!--x-->";
        assert_eq!(serialize_html(&parse_html(source)), source);
    }

    #[test]
    fn unquoted_attributes_are_accepted() {
        let nodes = parse_html("<a href=javascript:alert(1)>x</a>");
        assert_eq!(
            nodes[0],
            HtmlNode::Element {
                tag: "a".to_owned(),
                attrs: vec![("href".to_owned(), "javascript:alert(1)".to_owned())],
                children: vec![HtmlNode::Text("x".to_owned())],
            }
        );
    }
}
