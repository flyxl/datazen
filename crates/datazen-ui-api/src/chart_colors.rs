use std::collections::BTreeMap;

pub const COLOR_PALETTES: [(&str, &[&str]); 8] = [
    (
        "default",
        &["#60a5fa", "#34d399", "#fbbf24", "#f87171", "#a78bfa", "#22d3ee", "#fb923c", "#f472b6"],
    ),
    (
        "workspace",
        &["#2563eb", "#60a5fa", "#93c5fd", "#34d399", "#5eead4", "#fbbf24", "#f87171", "#a78bfa"],
    ),
    (
        "navy",
        &["#0b1220", "#111827", "#182231", "#202b3a", "#374151", "#6b7280", "#9ca3af", "#d1d5db"],
    ),
    ("status", &["#34d399", "#fbbf24", "#f87171", "#60a5fa", "#9ca3af"]),
    (
        "ocean",
        &["#0ea5e9", "#0284c7", "#0369a1", "#075985", "#0c4a6e", "#164e63", "#155e75", "#0e7490"],
    ),
    (
        "forest",
        &["#22c55e", "#16a34a", "#15803d", "#166534", "#14532d", "#365314", "#3f6212", "#4d7c0f"],
    ),
    (
        "warm",
        &["#f97316", "#ef4444", "#f59e0b", "#ec4899", "#e11d48", "#be123c", "#9f1239", "#881337"],
    ),
    (
        "mono",
        &["#6b7280", "#4b5563", "#374151", "#1f2937", "#9ca3af", "#d1d5db", "#e5e7eb", "#f3f4f6"],
    ),
];

pub const STROKE_DASH_PATTERNS: [Option<&str>; 6] = [
    None,
    Some("8 4"),
    Some("2 4"),
    Some("8 4 2 4"),
    Some("12 4"),
    Some("4 4"),
];

pub fn built_in_palette(name: &str) -> Option<&'static [&'static str]> {
    COLOR_PALETTES
        .iter()
        .find(|(palette_name, _)| *palette_name == name)
        .map(|(_, colors)| *colors)
}

pub fn get_color_palette(
    name: &str,
    override_palettes: Option<&BTreeMap<String, Vec<String>>>,
) -> Vec<String> {
    if let Some(overrides) = override_palettes {
        if let Some(value) = overrides.get(name) {
            if !value.is_empty() {
                return value.clone();
            }
        }
    }
    let selected = built_in_palette(name)
        .unwrap_or_else(|| built_in_palette("default").unwrap_or(&[] as &[&str]));
    selected.iter().map(|color| (*color).to_owned()).collect()
}

pub fn read_css_color(value: Option<&str>, fallback: &str) -> String {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn override_map(entries: &[(&str, Vec<String>)]) -> BTreeMap<String, Vec<String>> {
        entries.iter().map(|(name, colors)| ((*name).to_owned(), colors.clone())).collect()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn the_eight_palettes_keep_their_names_order_and_lengths() {
        let names: Vec<&str> = COLOR_PALETTES.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec!["default", "workspace", "navy", "status", "ocean", "forest", "warm", "mono"]
        );
        let lengths: Vec<usize> = COLOR_PALETTES.iter().map(|(_, colors)| colors.len()).collect();
        assert_eq!(lengths, vec![8, 8, 8, 5, 8, 8, 8, 8]);
        for (_, colors) in COLOR_PALETTES {
            assert!(colors.iter().all(|color| color.starts_with('#')));
        }
    }

    #[test]
    fn every_palette_pins_its_exact_hex_literals() {
        assert_eq!(
            built_in_palette("default"),
            Some(&["#60a5fa", "#34d399", "#fbbf24", "#f87171", "#a78bfa", "#22d3ee", "#fb923c", "#f472b6"][..])
        );
        assert_eq!(
            built_in_palette("workspace"),
            Some(&["#2563eb", "#60a5fa", "#93c5fd", "#34d399", "#5eead4", "#fbbf24", "#f87171", "#a78bfa"][..])
        );
        assert_eq!(
            built_in_palette("navy"),
            Some(&["#0b1220", "#111827", "#182231", "#202b3a", "#374151", "#6b7280", "#9ca3af", "#d1d5db"][..])
        );
        assert_eq!(
            built_in_palette("status"),
            Some(&["#34d399", "#fbbf24", "#f87171", "#60a5fa", "#9ca3af"][..])
        );
        assert_eq!(
            built_in_palette("ocean"),
            Some(&["#0ea5e9", "#0284c7", "#0369a1", "#075985", "#0c4a6e", "#164e63", "#155e75", "#0e7490"][..])
        );
        assert_eq!(
            built_in_palette("forest"),
            Some(&["#22c55e", "#16a34a", "#15803d", "#166534", "#14532d", "#365314", "#3f6212", "#4d7c0f"][..])
        );
        assert_eq!(
            built_in_palette("warm"),
            Some(&["#f97316", "#ef4444", "#f59e0b", "#ec4899", "#e11d48", "#be123c", "#9f1239", "#881337"][..])
        );
        assert_eq!(
            built_in_palette("mono"),
            Some(&["#6b7280", "#4b5563", "#374151", "#1f2937", "#9ca3af", "#d1d5db", "#e5e7eb", "#f3f4f6"][..])
        );
    }

    #[test]
    fn an_unknown_palette_falls_back_to_default() {
        for name in ["", "missing", "DEFAULT", "status "] {
            assert_eq!(
                get_color_palette(name, None),
                strings(&["#60a5fa", "#34d399", "#fbbf24", "#f87171", "#a78bfa", "#22d3ee", "#fb923c", "#f472b6"]),
                "{name}"
            );
        }
    }

    #[test]
    fn a_built_in_palette_is_returned_without_an_override() {
        assert_eq!(
            get_color_palette("status", None),
            strings(&["#34d399", "#fbbf24", "#f87171", "#60a5fa", "#9ca3af"])
        );
    }

    #[test]
    fn a_non_empty_override_wins_for_its_name_only() {
        let overrides = override_map(&[("status", strings(&["#111111", "#222222"]))]);
        assert_eq!(get_color_palette("status", Some(&overrides)), strings(&["#111111", "#222222"]));
        assert_eq!(get_color_palette("ocean", Some(&overrides)).len(), 8);
        assert_eq!(get_color_palette("status", None).len(), 5);
    }

    #[test]
    fn an_empty_override_falls_through_to_the_built_in_palette() {
        let overrides = override_map(&[("status", Vec::new()), ("missing", Vec::new())]);
        assert_eq!(
            get_color_palette("status", Some(&overrides)),
            strings(&["#34d399", "#fbbf24", "#f87171", "#60a5fa", "#9ca3af"])
        );
        assert_eq!(
            get_color_palette("missing", Some(&overrides)),
            strings(&["#60a5fa", "#34d399", "#fbbf24", "#f87171", "#a78bfa", "#22d3ee", "#fb923c", "#f472b6"])
        );
    }

    #[test]
    fn an_override_never_changes_the_built_in_table() {
        let overrides = override_map(&[("default", strings(&["#000000"]))]);
        assert_eq!(get_color_palette("default", Some(&overrides)), strings(&["#000000"]));
        assert_eq!(built_in_palette("default").map(|colors| colors.len()), Some(8));
        assert_eq!(get_color_palette("default", None).len(), 8);
    }

    #[test]
    fn the_dash_ladder_starts_with_solid_and_keeps_five_patterns() {
        assert_eq!(
            STROKE_DASH_PATTERNS,
            [None, Some("8 4"), Some("2 4"), Some("8 4 2 4"), Some("12 4"), Some("4 4")]
        );
    }

    #[test]
    fn a_computed_css_value_is_trimmed_and_falls_back_when_empty() {
        assert_eq!(read_css_color(Some("  #ff0000  "), "#000000"), "#ff0000");
        assert_eq!(read_css_color(Some("   "), "#000000"), "#000000");
        assert_eq!(read_css_color(Some(""), "#000000"), "#000000");
        assert_eq!(read_css_color(None, "#000000"), "#000000");
    }
}
