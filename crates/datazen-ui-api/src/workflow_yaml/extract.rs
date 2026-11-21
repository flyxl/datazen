fn find_key(yaml: &str, key: &str) -> bool {
    let bytes = yaml.as_bytes();
    let needle = key.as_bytes();
    let mut index = 0usize;
    while index + needle.len() <= bytes.len() {
        if &bytes[index..index + needle.len()] == needle {
            let before_ok = index == 0
                || !(bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_');
            if before_ok {
                let mut cursor = index + needle.len();
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len() && bytes[cursor] == b':' {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

pub fn looks_like_workflow(yaml: &str) -> bool {
    let has_body = find_key(yaml, "steps") || find_key(yaml, "blocks");
    has_body && (find_key(yaml, "id") || find_key(yaml, "name"))
}

fn opening_fence(text: &str) -> Option<(usize, usize)> {
    let yaml = text.find("```yaml\n").map(|start| (start, start + 8));
    let yml = text.find("```yml\n").map(|start| (start, start + 7));
    match (yaml, yml) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub fn extract_workflow_yaml(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = text;
    loop {
        let (_, content_start) = match opening_fence(rest) {
            Some(fence) => fence,
            None => break,
        };
        let after = &rest[content_start..];
        let end = match after.find("```") {
            Some(end) => end,
            None => break,
        };
        let content = after[..end].trim().to_owned();
        if looks_like_workflow(&content) {
            blocks.push(content);
        }
        rest = &after[end + 3..];
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_detection_respects_word_boundaries_and_whitespace() {
        assert!(find_key("steps:\n", "steps"));
        assert!(find_key("  blocks  : x", "blocks"));
        assert!(!find_key("mysteps: x", "steps"));
        assert!(!find_key("steps x", "steps"));
    }

    #[test]
    fn workflow_look_needs_a_body_and_an_identity() {
        assert!(looks_like_workflow("id: w1\nsteps:\n  - id: s1"));
        assert!(looks_like_workflow("name: w\nblocks:\n  - id: b1"));
        assert!(!looks_like_workflow("id: w1\nfoo: bar"));
        assert!(!looks_like_workflow("steps:\n  - type: query"));
    }

    #[test]
    fn extracts_only_looking_like_workflow_fences() {
        let text = "before\n```yaml\nname: a\nsteps:\n  - id: s\n```\ntext\n```yml\nname: b\nblocks:\n  - id: b1\n```\n```yaml\nhello: world\n```";
        let blocks = extract_workflow_yaml(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "name: a\nsteps:\n  - id: s");
        assert_eq!(blocks[1], "name: b\nblocks:\n  - id: b1");
    }

    #[test]
    fn unclosed_fences_are_ignored() {
        assert!(extract_workflow_yaml("```yaml\nid: x\nsteps:\n").is_empty());
    }
}
