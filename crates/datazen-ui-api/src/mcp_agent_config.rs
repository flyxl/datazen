use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpAgentTarget {
    Cursor,
    Claude,
}

impl McpAgentTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Claude => "claude",
        }
    }

    pub fn config_path_hint(self) -> &'static str {
        match self {
            Self::Cursor => ".cursor/mcp.json (project) or Cursor Settings → MCP",
            Self::Claude => "claude_desktop_config.json (Claude Desktop app support directory)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct McpServerEntry {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct McpServers {
    datazen: McpServerEntry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct McpWrapper {
    #[serde(rename = "mcpServers")]
    mcp_servers: McpServers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAgentSnippet {
    pub target: McpAgentTarget,
    pub config_path_hint: String,
    pub json: String,
}

pub fn build_mcp_server_entry(command: &str) -> McpServerEntry {
    McpServerEntry {
        command: command.to_owned(),
        args: vec!["--mcp".to_owned()],
    }
}

pub fn format_mcp_cli_command(command: &str) -> String {
    let trimmed = command.trim();
    let fallback = if trimmed.is_empty() { "datazen" } else { trimmed };
    let bytes = fallback.as_bytes();
    let has_quotes = (bytes.first() == Some(&b'"') && bytes.last() == Some(&b'"'))
        || (bytes.first() == Some(&b'\'') && bytes.last() == Some(&b'\''));
    let quoted = if fallback.contains(' ') && !has_quotes {
        format!("\"{fallback}\"")
    } else {
        fallback.to_owned()
    };
    format!("{quoted} --mcp")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppExecutablePathResolver {
    pub cached: Option<String>,
}

impl AppExecutablePathResolver {
    pub fn resolve(&mut self, mut read: impl FnMut() -> Result<Option<String>, String>) -> String {
        if let Some(path) = self.cached.as_ref() {
            return path.clone();
        }
        match read() {
            Ok(Some(path)) if !path.trim().is_empty() => {
                let trimmed = path.trim().to_owned();
                self.cached = Some(trimmed.clone());
                trimmed
            }
            _ => "datazen".to_owned(),
        }
    }

    pub fn clear(&mut self) {
        self.cached = None;
    }
}

pub fn initial_app_executable_path(cached: Option<&str>) -> String {
    cached.unwrap_or("datazen").to_owned()
}

pub fn build_mcp_agent_snippet(target: McpAgentTarget, command: &str) -> McpAgentSnippet {
    let wrapper = McpWrapper {
        mcp_servers: McpServers {
            datazen: build_mcp_server_entry(command),
        },
    };
    McpAgentSnippet {
        target,
        config_path_hint: target.config_path_hint().to_owned(),
        json: serde_json::to_string_pretty(&wrapper).unwrap_or_default(),
    }
}

pub fn build_mcp_agent_snippet_default(target: McpAgentTarget) -> McpAgentSnippet {
    build_mcp_agent_snippet(target, "datazen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_literals_and_path_hints() {
        assert_eq!(McpAgentTarget::Cursor.as_str(), "cursor");
        assert_eq!(McpAgentTarget::Claude.as_str(), "claude");
        assert_eq!(McpAgentTarget::Cursor.config_path_hint(), ".cursor/mcp.json (project) or Cursor Settings → MCP");
        assert_eq!(McpAgentTarget::Claude.config_path_hint(), "claude_desktop_config.json (Claude Desktop app support directory)");
    }

    #[test]
    fn server_entry_is_command_plus_one_mcp_flag() {
        assert_eq!(
            build_mcp_server_entry("/opt/bin/datazen"),
            McpServerEntry { command: "/opt/bin/datazen".to_owned(), args: vec!["--mcp".to_owned()] }
        );
    }

    #[test]
    fn cli_command_trims_defaults_and_quotes_spaced_paths() {
        assert_eq!(format_mcp_cli_command("datazen"), "datazen --mcp");
        assert_eq!(format_mcp_cli_command("   "), "datazen --mcp");
        assert_eq!(format_mcp_cli_command("  /opt/my app/datazen  "), "\"/opt/my app/datazen\" --mcp");
        assert_eq!(format_mcp_cli_command("\"/opt/my app/datazen\""), "\"/opt/my app/datazen\" --mcp");
        assert_eq!(format_mcp_cli_command("'/opt/my app/datazen'"), "'/opt/my app/datazen' --mcp");
        assert_eq!(format_mcp_cli_command("/opt/datazen"), "/opt/datazen --mcp");
    }

    #[test]
    fn snippets_carry_the_target_hint_and_pretty_json() {
        let snippet = build_mcp_agent_snippet(McpAgentTarget::Cursor, "datazen");
        assert_eq!(snippet.target, McpAgentTarget::Cursor);
        assert_eq!(snippet.config_path_hint, ".cursor/mcp.json (project) or Cursor Settings → MCP");
        assert_eq!(
            snippet.json,
            "{\n  \"mcpServers\": {\n    \"datazen\": {\n      \"command\": \"datazen\",\n      \"args\": [\n        \"--mcp\"\n      ]\n    }\n  }\n}"
        );
        let claude = build_mcp_agent_snippet(McpAgentTarget::Claude, "/opt/bin/datazen");
        assert!(claude.json.contains("\"/opt/bin/datazen\""));
        assert!(claude.config_path_hint.contains("claude_desktop_config.json"));
    }

    #[test]
    fn a_non_empty_read_is_trimmed_cached_and_reused() {
        let mut resolver = AppExecutablePathResolver::default();
        let mut reads = 0;
        let first = resolver.resolve(|| {
            reads += 1;
            Ok(Some("  /opt/bin/datazen  ".to_owned()))
        });
        assert_eq!(first, "/opt/bin/datazen");
        assert_eq!(resolver.cached.as_deref(), Some("/opt/bin/datazen"));
        let second = resolver.resolve(|| {
            reads += 1;
            Ok(Some("other".to_owned()))
        });
        assert_eq!(second, "/opt/bin/datazen");
        assert_eq!(reads, 1);
    }

    #[test]
    fn empty_failed_and_absent_reads_fall_back_to_the_default() {
        for result in [
            Ok(Some(String::new())),
            Ok(Some("   ".to_owned())),
            Ok(None),
            Err("non-tauri".to_owned()),
        ] {
            let mut resolver = AppExecutablePathResolver::default();
            assert_eq!(resolver.resolve(|| result.clone()), "datazen");
            assert_eq!(resolver.cached, None);
        }
    }

    #[test]
    fn a_read_returning_the_default_is_still_cached() {
        let mut resolver = AppExecutablePathResolver::default();
        assert_eq!(resolver.resolve(|| Ok(Some(" datazen ".to_owned()))), "datazen");
        assert_eq!(resolver.cached.as_deref(), Some("datazen"));
        assert_eq!(resolver.resolve(|| Ok(Some("/other".to_owned()))), "datazen");
    }

    #[test]
    fn the_default_snippet_uses_the_default_command() {
        assert_eq!(build_mcp_agent_snippet_default(McpAgentTarget::Cursor), build_mcp_agent_snippet(McpAgentTarget::Cursor, "datazen"));
    }

    #[test]
    fn clearing_the_cache_forces_a_fresh_read() {
        let mut resolver = AppExecutablePathResolver::default();
        assert_eq!(resolver.resolve(|| Ok(Some("/opt/bin/datazen".to_owned()))), "/opt/bin/datazen");
        resolver.clear();
        assert_eq!(resolver.cached, None);
        assert_eq!(resolver.resolve(|| Ok(Some("/other/datazen".to_owned()))), "/other/datazen");
    }

    #[test]
    fn the_hook_initial_value_comes_from_the_cache_or_default() {
        assert_eq!(initial_app_executable_path(None), "datazen");
        assert_eq!(initial_app_executable_path(Some("/opt/bin/datazen")), "/opt/bin/datazen");
    }
}
