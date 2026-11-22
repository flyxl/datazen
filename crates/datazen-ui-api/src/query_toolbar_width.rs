pub const TOOLBAR_GAP: i64 = 8;
pub const TOOLBAR_HORIZONTAL_PADDING: i64 = 32;
pub const QUERY_TOOLBAR_ICON_BUTTON_WIDTH: i64 = 32;
pub const QUERY_TOOLBAR_STATUS_WIDTH: i64 = 236;
pub const PATH_HIERARCHY_SELECT_MAX: i64 = 88;
pub const PATH_HIERARCHY_SELECT_MIN: i64 = 40;
pub const PATH_HIERARCHY_LABEL_MAX: i64 = 72;
pub const PATH_HIERARCHY_SELECT_CHROME: i64 = 28;
pub const PATH_HIERARCHY_SEGMENT_GAP: i64 = 8;
pub const PATH_HIERARCHY_PLACEHOLDER_CHARS: usize = 6;

const CHAR_PX: f64 = 6.5;

pub const QUERY_TOOLBAR_WIDTH_SURFACE: [&str; 3] = [
    "queryToolbarExpandedMinWidth",
    "queryContextSelectWidth",
    "pathHierarchyLabelWidth",
];

pub const QUERY_TOOLBAR_WIDTH_GAP: [&str; 3] = [
    "pathHierarchySelectorSegmentsForUi over SqlNamespace and path aliases",
    "useCompactToolbar's DOM measurement of the expanded toolbar",
    "the React render that applies the computed minimum width",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorSegmentKind {
    Label,
    Select,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathSelectorSegment {
    pub kind: SelectorSegmentKind,
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolbarWidthOptions {
    pub has_context_selectors: bool,
    pub is_path_hierarchy: bool,
    pub is_multi_db: bool,
    pub in_transaction: bool,
    pub context_schema: Option<String>,
    pub segments: Vec<PathSelectorSegment>,
    pub databases: Vec<String>,
    pub current_database: Option<String>,
    pub supports_explain: bool,
}

pub fn query_context_select_width(value: &str) -> i64 {
    let len = if value.is_empty() {
        PATH_HIERARCHY_PLACEHOLDER_CHARS
    } else {
        value.chars().count()
    };
    let measured = ((len as f64 * CHAR_PX) + PATH_HIERARCHY_SELECT_CHROME as f64).ceil() as i64;
    measured.clamp(PATH_HIERARCHY_SELECT_MIN, PATH_HIERARCHY_SELECT_MAX)
}

pub fn path_hierarchy_label_width(name: &str) -> i64 {
    let measured = (name.chars().count() as f64 * CHAR_PX).ceil() as i64;
    measured.clamp(24, PATH_HIERARCHY_LABEL_MAX)
}

pub fn query_toolbar_expanded_min_width(options: &ToolbarWidthOptions) -> i64 {
    let button_count = 10
        + if options.supports_explain { 1 } else { 0 }
        + 1
        + if options.in_transaction { 1 } else { 0 };

    let mut context_width = 0i64;
    if options.has_context_selectors {
        if options.is_path_hierarchy {
            let segment_width: i64 = options
                .segments
                .iter()
                .map(|segment| match segment.kind {
                    SelectorSegmentKind::Label => path_hierarchy_label_width(&segment.name),
                    SelectorSegmentKind::Select => query_context_select_width(&segment.value),
                })
                .sum();
            let gaps = (options.segments.len() as i64 - 1).max(0) * PATH_HIERARCHY_SEGMENT_GAP;
            context_width = 20 + segment_width.max(PATH_HIERARCHY_SELECT_MIN * 2) + gaps;
        } else if options.is_multi_db {
            let selected = options
                .current_database
                .as_deref()
                .or_else(|| options.databases.first().map(String::as_str))
                .unwrap_or("");
            context_width = 20 + query_context_select_width(selected).max(PATH_HIERARCHY_SELECT_MIN * 2);
        } else if let Some(schema) = options.context_schema.as_deref().filter(|value| !value.is_empty()) {
            let _ = schema;
            context_width = 96;
        }
    }

    TOOLBAR_HORIZONTAL_PADDING
        + button_count * QUERY_TOOLBAR_ICON_BUTTON_WIDTH
        + (button_count - 1).max(0) * TOOLBAR_GAP
        + QUERY_TOOLBAR_STATUS_WIDTH
        + context_width
        + 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_width_is_clamped_and_uses_a_placeholder_for_empty_values() {
        assert_eq!(query_context_select_width(""), 67, "6 chars * 6.5 + 28 = 67");
        assert_eq!(query_context_select_width("a"), 40, "a short value clamps to the minimum");
        assert_eq!(query_context_select_width("abcdefghijk"), 88, "a long value clamps to the maximum");
    }

    #[test]
    fn label_width_is_clamped() {
        assert_eq!(path_hierarchy_label_width(""), 24);
        assert_eq!(path_hierarchy_label_width("a"), 24);
        assert_eq!(path_hierarchy_label_width("abcdefghijklmno"), 72);
    }

    #[test]
    fn the_base_budget_counts_buttons_status_and_padding() {
        let options = ToolbarWidthOptions::default();
        let base = query_toolbar_expanded_min_width(&options);
        assert_eq!(base, 32 + 11 * 32 + 10 * 8 + 236 + 0 + 8, "10 buttons + one Pro slot");
        let explain = ToolbarWidthOptions { supports_explain: true, ..Default::default() };
        assert_eq!(query_toolbar_expanded_min_width(&explain), base + 32 + 8);
        let tx = ToolbarWidthOptions { in_transaction: true, ..Default::default() };
        assert_eq!(query_toolbar_expanded_min_width(&tx), base + 32 + 8);
    }

    #[test]
    fn multi_db_and_schema_selectors_add_their_context_width() {
        let multi = ToolbarWidthOptions {
            has_context_selectors: true,
            is_multi_db: true,
            databases: vec!["shop".to_owned()],
            ..Default::default()
        };
        let base = query_toolbar_expanded_min_width(&ToolbarWidthOptions::default());
        assert_eq!(query_toolbar_expanded_min_width(&multi), base + 20 + 80);

        let schema = ToolbarWidthOptions {
            has_context_selectors: true,
            context_schema: Some("public".to_owned()),
            ..Default::default()
        };
        assert_eq!(query_toolbar_expanded_min_width(&schema), base + 96);
        let empty_schema = ToolbarWidthOptions {
            has_context_selectors: true,
            context_schema: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(query_toolbar_expanded_min_width(&empty_schema), base);
    }

    #[test]
    fn path_hierarchy_sums_segment_widths_and_gaps() {
        let segments = vec![
            PathSelectorSegment { kind: SelectorSegmentKind::Label, name: "db".to_owned(), value: String::new() },
            PathSelectorSegment { kind: SelectorSegmentKind::Select, name: String::new(), value: "schema".to_owned() },
        ];
        let hierarchy = ToolbarWidthOptions {
            has_context_selectors: true,
            is_path_hierarchy: true,
            segments,
            ..Default::default()
        };
        let base = query_toolbar_expanded_min_width(&ToolbarWidthOptions::default());
        let expected = 20 + 24 + 67 + 8;
        assert_eq!(query_toolbar_expanded_min_width(&hierarchy), base + expected);
    }

    #[test]
    fn surface_and_gap_are_pinned() {
        assert_eq!(QUERY_TOOLBAR_WIDTH_SURFACE.len(), 3);
        assert_eq!(QUERY_TOOLBAR_WIDTH_GAP.len(), 3);
        assert_eq!(crate::QUERY_TOOLBAR_WIDTH_SURFACE.len(), 3);
        assert_eq!(crate::QUERY_TOOLBAR_WIDTH_GAP.len(), 3);
    }
}
