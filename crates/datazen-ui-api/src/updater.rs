use serde_json::Value;

use crate::CHANNEL_AUTO;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateChannel {
    Auto,
    Manual,
    None,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => CHANNEL_AUTO,
            Self::Manual => "manual",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdaterBuildConfig {
    pub is_desktop_build: bool,
    pub has_updater_channel: bool,
    pub variant: String,
}

pub fn get_update_channel(config: &UpdaterBuildConfig) -> UpdateChannel {
    if !config.is_desktop_build {
        return UpdateChannel::None;
    }
    if config.has_updater_channel {
        UpdateChannel::Auto
    } else {
        UpdateChannel::Manual
    }
}

pub fn current_variant(config: &UpdaterBuildConfig) -> &str {
    &config.variant
}

pub fn has_updater_channel(config: &UpdaterBuildConfig) -> bool {
    config.has_updater_channel
}

pub fn is_updater_supported(config: &UpdaterBuildConfig) -> bool {
    get_update_channel(config) == UpdateChannel::Auto
}

pub fn normalize_sku(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .trim_start_matches('-')
        .to_owned()
}

pub fn read_manifest_variant(raw_json: &Value) -> Option<String> {
    let object = raw_json.as_object()?;
    let variant = object.get("variant")?.as_str()?.trim();
    if variant.is_empty() {
        None
    } else {
        Some(variant.to_owned())
    }
}

pub fn manifest_belongs_to_build(raw_json: &Value, variant: &str) -> bool {
    match read_manifest_variant(raw_json) {
        None => normalize_sku(variant) == "basic",
        Some(found) => normalize_sku(&found) == normalize_sku(variant),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateProgress {
    Checking,
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
    Done { version: String },
    Idle,
}

impl UpdateProgress {
    pub fn phase(self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Downloading { .. } => "downloading",
            Self::Installing => "installing",
            Self::Done { .. } => "done",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckResult {
    UpToDate,
    Available { version: String },
    Installed { version: String },
    Error { message: String },
}

impl UpdateCheckResult {
    pub fn status(&self) -> &'static str {
        match self {
            Self::UpToDate => "upToDate",
            Self::Available { .. } => "available",
            Self::Installed { .. } => "installed",
            Self::Error { .. } => "error",
        }
    }
}

pub fn check_for_updates<C>(check: C) -> UpdateCheckResult
where
    C: FnOnce() -> UpdateCheckResult,
{
    check()
}

pub fn download_and_install_update<P>(
    on_progress: Option<&dyn Fn(&UpdateProgress)>,
    install: P,
) -> UpdateCheckResult
where
    P: FnOnce(Option<&dyn Fn(&UpdateProgress)>) -> UpdateCheckResult,
{
    install(on_progress)
}

pub fn relaunch_to_apply_update<R>(relaunch: R)
where
    R: FnOnce(),
{
    relaunch()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdaterLogLevel {
    Info,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdaterLog {
    pub level: UpdaterLogLevel,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartupCheckPlan {
    pub channel: UpdateChannel,
    pub enabled: bool,
    pub variant: String,
    pub install_called: bool,
    pub install_result: Option<UpdateCheckResult>,
    pub logs: Vec<UpdaterLog>,
}

pub fn maybe_check_on_startup<F>(
    config: &UpdaterBuildConfig,
    enabled: bool,
    download_install: F,
) -> StartupCheckPlan
where
    F: FnOnce() -> UpdateCheckResult,
{
    let channel = get_update_channel(config);
    let variant = current_variant(config).to_owned();
    if !enabled || channel != UpdateChannel::Auto {
        let message = format!(
            "[updater] startup check skipped: setting={enabled} channel={} variant={variant}",
            channel.as_str()
        );
        return StartupCheckPlan {
            channel,
            enabled,
            variant,
            install_called: false,
            install_result: None,
            logs: vec![UpdaterLog {
                level: UpdaterLogLevel::Info,
                message,
            }],
        };
    }

    let result = download_install();
    let mut logs = Vec::new();
    if let UpdateCheckResult::Error { message } = &result {
        logs.push(UpdaterLog {
            level: UpdaterLogLevel::Warn,
            message: format!("[updater] startup check failed: {message}"),
        });
    }
    StartupCheckPlan {
        channel,
        enabled,
        variant,
        install_called: true,
        install_result: Some(result),
        logs,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn config(desktop: bool, channel: bool) -> UpdaterBuildConfig {
        UpdaterBuildConfig {
            is_desktop_build: desktop,
            has_updater_channel: channel,
            variant: "basic".to_owned(),
        }
    }

    #[test]
    fn channel_is_none_off_desktop_and_auto_or_manual_on_desktop() {
        assert_eq!(
            get_update_channel(&config(false, true)),
            UpdateChannel::None
        );
        assert_eq!(
            get_update_channel(&config(false, false)),
            UpdateChannel::None
        );
        assert_eq!(get_update_channel(&config(true, true)), UpdateChannel::Auto);
        assert_eq!(
            get_update_channel(&config(true, false)),
            UpdateChannel::Manual
        );
        assert_eq!(UpdateChannel::Auto.as_str(), "auto");
        assert_eq!(UpdateChannel::Manual.as_str(), "manual");
        assert_eq!(UpdateChannel::None.as_str(), "none");
    }

    #[test]
    fn variant_and_support_follow_the_channel() {
        let mut item = config(true, true);
        assert_eq!(current_variant(&item), "basic");
        assert!(has_updater_channel(&item));
        assert!(is_updater_supported(&item));
        item.variant = "all".to_owned();
        assert_eq!(current_variant(&item), "all");
        assert!(!is_updater_supported(&config(false, true)));
        assert!(!is_updater_supported(&config(true, false)));
    }

    #[test]
    fn sku_normalization_strips_case_whitespace_and_leading_dashes() {
        assert_eq!(normalize_sku("  ALL "), "all");
        assert_eq!(normalize_sku("--Akulaku"), "akulaku");
        assert_eq!(normalize_sku("-all"), "all");
    }

    #[test]
    fn manifest_variant_reads_only_nonblank_strings() {
        assert_eq!(
            read_manifest_variant(&json!({"variant": " all "})).as_deref(),
            Some("all")
        );
        assert_eq!(read_manifest_variant(&json!({"variant": "  "})), None);
        assert_eq!(read_manifest_variant(&json!({"variant": 3})), None);
        assert_eq!(read_manifest_variant(&json!(null)), None);
        assert_eq!(read_manifest_variant(&json!([])), None);
    }

    #[test]
    fn manifest_defence_matches_sku_and_keeps_legacy_basic() {
        assert!(manifest_belongs_to_build(&json!({"variant": "all"}), "all"));
        assert!(manifest_belongs_to_build(
            &json!({"variant": "-all"}),
            "all"
        ));
        assert!(!manifest_belongs_to_build(
            &json!({"variant": "all"}),
            "basic"
        ));
        assert!(manifest_belongs_to_build(&json!({}), "basic"));
        assert!(!manifest_belongs_to_build(&json!({}), "all"));
    }

    #[test]
    fn result_and_progress_phases_are_pinned() {
        assert_eq!(UpdateCheckResult::UpToDate.status(), "upToDate");
        assert_eq!(
            UpdateCheckResult::Available {
                version: "1".to_owned()
            }
            .status(),
            "available"
        );
        assert_eq!(
            UpdateCheckResult::Installed {
                version: "1".to_owned()
            }
            .status(),
            "installed"
        );
        assert_eq!(
            UpdateCheckResult::Error {
                message: "x".to_owned()
            }
            .status(),
            "error"
        );
        assert_eq!(UpdateProgress::Checking.phase(), "checking");
        assert_eq!(
            UpdateProgress::Downloading {
                downloaded: 1,
                total: None
            }
            .phase(),
            "downloading"
        );
        assert_eq!(UpdateProgress::Installing.phase(), "installing");
        assert_eq!(
            UpdateProgress::Done {
                version: "1".to_owned()
            }
            .phase(),
            "done"
        );
        assert_eq!(UpdateProgress::Idle.phase(), "idle");
    }

    #[test]
    fn port_accessors_delegate_to_the_injected_operations() {
        let result = check_for_updates(|| UpdateCheckResult::UpToDate);
        assert_eq!(result, UpdateCheckResult::UpToDate);
        let result = download_and_install_update(None, |callback| {
            assert!(callback.is_none());
            UpdateCheckResult::Installed {
                version: "2".to_owned(),
            }
        });
        assert_eq!(result.status(), "installed");
        let called = std::cell::Cell::new(false);
        relaunch_to_apply_update(|| called.set(true));
        assert!(called.get());
    }

    #[test]
    fn startup_check_skips_with_a_diagnosable_log() {
        let plan = maybe_check_on_startup(&config(true, true), false, || {
            panic!("install must not run when disabled")
        });
        assert_eq!(plan.channel, UpdateChannel::Auto);
        assert!(!plan.install_called);
        assert_eq!(plan.install_result, None);
        assert_eq!(plan.logs.len(), 1);
        assert_eq!(plan.logs[0].level, UpdaterLogLevel::Info);
        assert_eq!(
            plan.logs[0].message,
            "[updater] startup check skipped: setting=false channel=auto variant=basic"
        );

        let plan = maybe_check_on_startup(&config(false, true), true, || {
            panic!("install must not run off desktop")
        });
        assert_eq!(plan.channel, UpdateChannel::None);
        assert!(!plan.install_called);
    }

    #[test]
    fn startup_check_runs_only_when_auto_and_enabled() {
        let plan =
            maybe_check_on_startup(&config(true, true), true, || UpdateCheckResult::UpToDate);
        assert!(plan.install_called);
        assert_eq!(plan.install_result, Some(UpdateCheckResult::UpToDate));
        assert!(plan.logs.is_empty());

        let plan = maybe_check_on_startup(&config(true, true), true, || UpdateCheckResult::Error {
            message: "boom".to_owned(),
        });
        assert_eq!(plan.logs.len(), 1);
        assert_eq!(plan.logs[0].level, UpdaterLogLevel::Warn);
        assert_eq!(plan.logs[0].message, "[updater] startup check failed: boom");
    }
}
