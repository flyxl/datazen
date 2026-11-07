//! Static metadata for the eight desktop local CLI agents.

use super::local_cli::LocalCliPreset;
use crate::ai::domain::types::AiProviderType;

pub static LOCAL_CLI_PRESETS: &[LocalCliPreset] = &[
    LocalCliPreset {
        id: "claude_code",
        provider_type: AiProviderType::ClaudeCode,
        display_name: "Claude Code",
        command: "claude",
        executable_names: &["claude"],
        args: &["-p", "--output-format", "text"],
        default_model: "",
        install_hint: "Install Claude Code and run `claude` once to sign in.",
    },
    LocalCliPreset {
        id: "codex",
        provider_type: AiProviderType::Codex,
        display_name: "Codex",
        command: "codex",
        executable_names: &["codex"],
        args: &["exec", "--skip-git-repo-check"],
        default_model: "",
        install_hint: "Install the Codex CLI and run `codex` once to sign in.",
    },
    LocalCliPreset {
        id: "cursor",
        provider_type: AiProviderType::Cursor,
        display_name: "Cursor",
        command: "cursor-agent",
        executable_names: &["cursor-agent", "cursor"],
        args: &["-p"],
        default_model: "",
        install_hint: "Install the Cursor Agent CLI (`cursor-agent`).",
    },
    LocalCliPreset {
        id: "opencode",
        provider_type: AiProviderType::OpenCode,
        display_name: "OpenCode",
        command: "opencode",
        executable_names: &["opencode"],
        args: &["run"],
        default_model: "",
        install_hint: "Install OpenCode and make `opencode` available in PATH.",
    },
    LocalCliPreset {
        id: "grok",
        provider_type: AiProviderType::Grok,
        display_name: "Grok",
        command: "grok",
        executable_names: &["grok"],
        args: &[],
        default_model: "",
        install_hint: "Install the Grok CLI and make `grok` available in PATH.",
    },
    LocalCliPreset {
        id: "qoder",
        provider_type: AiProviderType::Qoder,
        display_name: "Qoder",
        command: "qoder",
        executable_names: &["qoder"],
        args: &[],
        default_model: "",
        install_hint: "Install Qoder and make `qoder` available in PATH.",
    },
    LocalCliPreset {
        id: "codebuddy",
        provider_type: AiProviderType::CodeBuddy,
        display_name: "CodeBuddy",
        command: "codebuddy",
        executable_names: &["codebuddy"],
        args: &[],
        default_model: "",
        install_hint: "Install CodeBuddy and make `codebuddy` available in PATH.",
    },
    LocalCliPreset {
        id: "pi",
        provider_type: AiProviderType::Pi,
        display_name: "Pi",
        command: "pi",
        executable_names: &["pi"],
        args: &[],
        default_model: "",
        install_hint: "Install Pi and make `pi` available in PATH.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::local_cli::preset_for_type;

    #[test]
    fn all_eight_presets_are_stable_and_unique() {
        assert_eq!(LOCAL_CLI_PRESETS.len(), 8);
        let mut ids = std::collections::HashSet::new();
        let mut types = std::collections::HashSet::new();
        for preset in LOCAL_CLI_PRESETS {
            assert!(ids.insert(preset.id), "duplicate id {}", preset.id);
            assert!(types.insert(format!("{:?}", preset.provider_type)));
            assert!(!preset.command.is_empty());
            assert!(!preset.display_name.is_empty());
            assert!(!preset.install_hint.is_empty());
            assert!(preset.executable_names.contains(&preset.command));
            assert!(preset.args.iter().all(|arg| !arg.is_empty()));
            assert!(!preset.args.iter().any(|arg| arg.contains("{{")));
        }
    }

    #[test]
    fn preset_ids_and_aliases_round_trip() {
        for (encoded, expected) in [
            ("\"claude_code\"", AiProviderType::ClaudeCode),
            ("\"codex\"", AiProviderType::Codex),
            ("\"cursor\"", AiProviderType::Cursor),
            ("\"opencode\"", AiProviderType::OpenCode),
            ("\"open_code\"", AiProviderType::OpenCode),
            ("\"grok\"", AiProviderType::Grok),
            ("\"qoder\"", AiProviderType::Qoder),
            ("\"codebuddy\"", AiProviderType::CodeBuddy),
            ("\"code_buddy\"", AiProviderType::CodeBuddy),
            ("\"pi\"", AiProviderType::Pi),
        ] {
            let parsed: AiProviderType = serde_json::from_str(encoded).unwrap();
            assert_eq!(parsed, expected, "{encoded}");
        }
    }

    #[test]
    fn every_preset_has_a_provider_type_mapping() {
        for preset in LOCAL_CLI_PRESETS {
            assert_eq!(
                preset_for_type(preset.provider_type).map(|p| p.id),
                Some(preset.id)
            );
        }
        assert!(preset_for_type(AiProviderType::OpenAi).is_none());
    }
}
