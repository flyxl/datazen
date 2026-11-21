pub const DEFAULT_SURFACE_DARK: &str = "#0b0e14";
pub const DEFAULT_SURFACE_LIGHT: &str = "#f8fafc";

pub const SURFACE_BG_CACHE_GAP: [&str; 3] = [
    "getPlatformOrNull()",
    "hostSettings.persistSurfaceBackground",
    "{appData}/surface-bg.json",
];

pub fn is_safe_css_hex(value: &str) -> bool {
    let digits = match value.trim().strip_prefix('#') {
        Some(digits) => digits,
        None => return false,
    };
    (digits.len() == 3 || digits.len() == 6) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_rgb_component(value: &str) -> Option<u32> {
    let digits: String = value
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

fn rgb_to_hex(trimmed: &str) -> Option<String> {
    let lower = trimmed.to_ascii_lowercase();
    let rest = match lower.strip_prefix("rgba(") {
        Some(rest) => rest,
        None => lower.strip_prefix("rgb(")?,
    };
    let mut components = rest.split(',');
    let red = parse_rgb_component(components.next()?)?;
    let green = parse_rgb_component(components.next()?)?;
    let blue = parse_rgb_component(components.next()?)?;
    Some(format!("#{red:02x}{green:02x}{blue:02x}"))
}

pub fn css_color_to_hex(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if is_safe_css_hex(trimmed) {
        let digits = trimmed.strip_prefix('#').unwrap_or("");
        if digits.len() == 3 {
            let mut expanded = String::with_capacity(7);
            expanded.push('#');
            for ch in digits.chars() {
                expanded.push(ch);
                expanded.push(ch);
            }
            return Some(expanded.to_lowercase());
        }
        return Some(trimmed.to_lowercase());
    }
    rgb_to_hex(trimmed)
}

pub fn persist_surface_background<F>(
    is_dark: bool,
    css_color: &str,
    persist: Option<F>,
) -> Option<String>
where
    F: Fn(&str, bool),
{
    let hex = css_color_to_hex(css_color)?;
    if let Some(persist) = persist {
        persist(&hex, is_dark);
    }
    Some(hex)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn defaults_and_gap_are_pinned() {
        assert_eq!(DEFAULT_SURFACE_DARK, "#0b0e14");
        assert_eq!(DEFAULT_SURFACE_LIGHT, "#f8fafc");
        assert_eq!(SURFACE_BG_CACHE_GAP.len(), 3);
        assert!(SURFACE_BG_CACHE_GAP.contains(&"hostSettings.persistSurfaceBackground"));
        assert_eq!(crate::SURFACE_BG_CACHE_GAP.len(), 3);
    }

    #[test]
    fn safe_hex_accepts_only_three_or_six_digits_with_a_hash() {
        assert!(is_safe_css_hex("#abc"));
        assert!(is_safe_css_hex("#ABCDEF"));
        assert!(is_safe_css_hex("  #aBcDeF  "));
        assert!(!is_safe_css_hex("abc"));
        assert!(!is_safe_css_hex("#ab"));
        assert!(!is_safe_css_hex("#abcd"));
        assert!(!is_safe_css_hex("#abcde"));
        assert!(!is_safe_css_hex("#abcdef12"));
        assert!(!is_safe_css_hex("#gg0000"));
        assert!(!is_safe_css_hex(""));
    }

    #[test]
    fn short_hex_expands_each_digit_and_lowercases() {
        assert_eq!(css_color_to_hex(" #AbC ").as_deref(), Some("#aabbcc"));
        assert_eq!(css_color_to_hex("#000").as_deref(), Some("#000000"));
        assert_eq!(css_color_to_hex("#FfF").as_deref(), Some("#ffffff"));
    }

    #[test]
    fn long_hex_is_lowercased_without_expansion() {
        assert_eq!(css_color_to_hex("#0B0E14").as_deref(), Some("#0b0e14"));
        assert_eq!(css_color_to_hex("#f8fafc").as_deref(), Some("#f8fafc"));
    }

    #[test]
    fn rgb_and_rgba_strings_convert_to_hex() {
        assert_eq!(
            css_color_to_hex("rgb(255, 0, 17)").as_deref(),
            Some("#ff0011")
        );
        assert_eq!(
            css_color_to_hex("RGBA(1,2,3,0.5)").as_deref(),
            Some("#010203")
        );
        assert_eq!(css_color_to_hex("  rgb (1,2,3)").as_deref(), None);
        assert_eq!(
            css_color_to_hex("rgb(007, 016, 0)").as_deref(),
            Some("#071000")
        );
        assert_eq!(
            css_color_to_hex("rgb(256,1,1)").as_deref(),
            Some("#1000101")
        );
    }

    #[test]
    fn malformed_colors_are_rejected() {
        for value in ["", "red", "rgb(1,2)", "rgb(a,2,3)", "#12345", "hsl(1,2,3)"] {
            assert_eq!(css_color_to_hex(value), None, "{value} is not convertible");
        }
    }

    #[test]
    fn persist_writes_only_the_converted_hex() {
        let calls = RefCell::new(Vec::new());
        let result = persist_surface_background(
            true,
            " rgb(10,20,30) ",
            Some(|hex: &str, dark: bool| {
                calls.borrow_mut().push((hex.to_owned(), dark));
            }),
        );
        assert_eq!(result.as_deref(), Some("#0a141e"));
        assert_eq!(calls.into_inner(), [("#0a141e".to_owned(), true)]);

        let calls = RefCell::new(Vec::new());
        let result = persist_surface_background(
            false,
            "red",
            Some(|hex: &str, dark: bool| {
                calls.borrow_mut().push((hex.to_owned(), dark));
            }),
        );
        assert_eq!(result, None);
        assert!(calls.borrow().is_empty());

        let result = persist_surface_background(false, "#abc", None::<fn(&str, bool)>);
        assert_eq!(result.as_deref(), Some("#aabbcc"));
    }
}
