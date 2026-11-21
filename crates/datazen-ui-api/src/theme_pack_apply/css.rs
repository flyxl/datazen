use super::ids::mime_for_path;

fn is_css_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn find_url_refs(css: &str) -> Vec<(usize, usize, String)> {
    let bytes = css.as_bytes();
    let mut refs = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if index + 3 <= bytes.len() && bytes[index..index + 3].eq_ignore_ascii_case(b"url") {
            let mut cursor = index + 3;
            while cursor < bytes.len() && is_css_whitespace(bytes[cursor]) {
                cursor += 1;
            }
            if cursor < bytes.len() && bytes[cursor] == b'(' {
                cursor += 1;
                while cursor < bytes.len() && is_css_whitespace(bytes[cursor]) {
                    cursor += 1;
                }
                let quote =
                    if cursor < bytes.len() && (bytes[cursor] == b'\'' || bytes[cursor] == b'"') {
                        let quote = bytes[cursor];
                        cursor += 1;
                        Some(quote)
                    } else {
                        None
                    };
                let value_start = cursor;
                while cursor < bytes.len() {
                    let byte = bytes[cursor];
                    if byte == b')' || byte == b'\'' || byte == b'"' {
                        break;
                    }
                    cursor += 1;
                }
                let value_end = cursor;
                let mut closed = false;
                if let Some(quote) = quote {
                    if cursor < bytes.len() && bytes[cursor] == quote {
                        cursor += 1;
                        while cursor < bytes.len() && is_css_whitespace(bytes[cursor]) {
                            cursor += 1;
                        }
                        closed = cursor < bytes.len() && bytes[cursor] == b')';
                        if closed {
                            cursor += 1;
                        }
                    }
                } else if cursor < bytes.len() && bytes[cursor] == b')' {
                    closed = true;
                    cursor += 1;
                }
                if closed && value_end > value_start {
                    refs.push((index, cursor, css[value_start..value_end].to_owned()));
                    index = cursor;
                    continue;
                }
            }
        }
        index += 1;
    }
    refs
}

pub fn rewrite_css_urls<F, B>(css: &str, read_file: F, blob_url: B) -> Result<String, String>
where
    F: Fn(&str) -> Option<Vec<u8>>,
    B: Fn(&[u8], &str, &str) -> String,
{
    let mut replacements = Vec::new();
    for (start, end, raw) in find_url_refs(css) {
        let reference = raw.trim();
        let lower = reference.to_lowercase();
        if lower.starts_with("http:") || lower.starts_with("https:") {
            return Err(format!("Remote font URL not allowed: {reference}"));
        }
        if reference.starts_with("blob:") || reference.starts_with("data:") {
            continue;
        }
        let relative_path = reference.strip_prefix("./").unwrap_or(reference);
        let bytes = match read_file(relative_path) {
            Some(bytes) => bytes,
            None => return Err(format!("Font file not found in pack: {relative_path}")),
        };
        let mime = mime_for_path(relative_path);
        let url = blob_url(&bytes, relative_path, mime);
        replacements.push((start, end, url));
    }
    if replacements.is_empty() {
        return Ok(css.to_owned());
    }
    let mut result = css.to_owned();
    for (start, end, url) in replacements.into_iter().rev() {
        result = format!("{}url(\"{}\"){}", &result[..start], url, &result[end..]);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_url(_: &[u8], path: &str, _: &str) -> String {
        format!("blob:{path}")
    }

    #[test]
    fn url_references_are_found_with_either_quote_or_none() {
        let refs = find_url_refs("url(a.png) URL(\"b.woff2\") uRl( 'c.svg' )");
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].2, "a.png");
        assert_eq!(refs[1].2, "b.woff2");
        assert_eq!(refs[2].2, "c.svg");
    }

    #[test]
    fn css_without_url_references_is_returned_unchanged() {
        let css = "body { color: red; }";
        assert_eq!(
            rewrite_css_urls(css, |_| None, make_url).as_deref(),
            Ok(css)
        );
    }

    #[test]
    fn local_files_become_blob_urls_in_reverse_replacement_order() {
        let css = "@font-face{src:url('./a.woff2')}b{background:url(\"img/b.png\")}";
        let result = rewrite_css_urls(
            css,
            |path| Some(format!("bytes:{path}").into_bytes()),
            make_url,
        )
        .unwrap();
        assert_eq!(
            result,
            "@font-face{src:url(\"blob:a.woff2\")}b{background:url(\"blob:img/b.png\")}"
        );
    }

    #[test]
    fn remote_urls_are_rejected_and_blob_data_are_kept() {
        let err = rewrite_css_urls("url(https://fonts/x.woff2)", |_| None, make_url);
        assert_eq!(
            err,
            Err("Remote font URL not allowed: https://fonts/x.woff2".to_owned())
        );
        let css = "a{background:url(blob:abc)}b{background:url(data:image/png;base64,AA)}";
        assert_eq!(
            rewrite_css_urls(css, |_| None, make_url).as_deref(),
            Ok(css)
        );
    }

    #[test]
    fn missing_local_files_fail_the_rewrite() {
        let err = rewrite_css_urls("url(missing.svg)", |_| None, make_url);
        assert_eq!(
            err,
            Err("Font file not found in pack: missing.svg".to_owned())
        );
    }
}
