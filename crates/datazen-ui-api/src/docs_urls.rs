pub const DOCS_BASE_EN: &str = "https://flyxl.github.io/datazen/manual.html";
pub const DOCS_BASE_ZH: &str = "https://flyxl.github.io/datazen/zh/manual.html";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocsSectionId {
    Overview,
    Features,
    Ai,
    Context,
    Workflows,
    OpsDashboard,
    DataSync,
    DataTransfer,
    SchemaDiff,
}

impl DocsSectionId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Features => "features",
            Self::Ai => "ai",
            Self::Context => "context",
            Self::Workflows => "workflows",
            Self::OpsDashboard => "opsDashboard",
            Self::DataSync => "dataSync",
            Self::DataTransfer => "dataTransfer",
            Self::SchemaDiff => "schemaDiff",
        }
    }

    pub fn anchor(self) -> &'static str {
        match self {
            Self::Overview => "ui",
            Self::Features => "charts",
            Self::Ai => "ai",
            Self::Context => "ai",
            Self::Workflows => "workflow",
            Self::OpsDashboard => "dashboard",
            Self::DataSync => "data-sync",
            Self::DataTransfer => "data-transfer",
            Self::SchemaDiff => "schema-diff",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "overview" => Some(Self::Overview),
            "features" => Some(Self::Features),
            "ai" => Some(Self::Ai),
            "context" => Some(Self::Context),
            "workflows" => Some(Self::Workflows),
            "opsDashboard" => Some(Self::OpsDashboard),
            "dataSync" => Some(Self::DataSync),
            "dataTransfer" => Some(Self::DataTransfer),
            "schemaDiff" => Some(Self::SchemaDiff),
            _ => None,
        }
    }
}

pub fn is_chinese_ui_language(language: &str) -> bool {
    language.starts_with("zh")
}

pub fn build_docs_url(language: &str, section: Option<&str>) -> String {
    let base = if is_chinese_ui_language(language) { DOCS_BASE_ZH } else { DOCS_BASE_EN };
    match section.map(str::trim).filter(|id| !id.is_empty()).and_then(DocsSectionId::parse) {
        Some(id) => format!("{base}#{}", id.anchor()),
        None => base.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_bases_are_the_official_manual_urls() {
        assert_eq!(DOCS_BASE_EN, "https://flyxl.github.io/datazen/manual.html");
        assert_eq!(DOCS_BASE_ZH, "https://flyxl.github.io/datazen/zh/manual.html");
    }

    #[test]
    fn chinese_detection_is_a_case_sensitive_prefix_test() {
        assert!(is_chinese_ui_language("zh"));
        assert!(is_chinese_ui_language("zh-CN"));
        assert!(is_chinese_ui_language("zhHans"));
        assert!(!is_chinese_ui_language("en"));
        assert!(!is_chinese_ui_language("ZH"));
        assert!(!is_chinese_ui_language("en-zh"));
    }

    #[test]
    fn every_legacy_section_maps_to_its_manual_anchor() {
        for (id, anchor) in [
            (DocsSectionId::Overview, "ui"),
            (DocsSectionId::Features, "charts"),
            (DocsSectionId::Ai, "ai"),
            (DocsSectionId::Context, "ai"),
            (DocsSectionId::Workflows, "workflow"),
            (DocsSectionId::OpsDashboard, "dashboard"),
            (DocsSectionId::DataSync, "data-sync"),
            (DocsSectionId::DataTransfer, "data-transfer"),
            (DocsSectionId::SchemaDiff, "schema-diff"),
        ] {
            assert_eq!(id.anchor(), anchor);
            assert_eq!(DocsSectionId::parse(id.as_str()), Some(id));
            assert_eq!(
                build_docs_url("en", Some(id.as_str())),
                format!("{DOCS_BASE_EN}#{anchor}")
            );
        }
    }

    #[test]
    fn a_known_section_is_trimmed_before_lookup() {
        assert_eq!(
            build_docs_url("zh-CN", Some("  dataSync  ")),
            format!("{DOCS_BASE_ZH}#data-sync")
        );
    }

    #[test]
    fn unknown_blank_and_absent_sections_return_the_bare_base() {
        for section in [None, Some(""), Some("   "), Some("unknown"), Some("datasync"), Some("OVERVIEW")] {
            assert_eq!(build_docs_url("en", section), DOCS_BASE_EN, "{section:?}");
            assert_eq!(build_docs_url("zh", section), DOCS_BASE_ZH, "{section:?}");
        }
    }
}
