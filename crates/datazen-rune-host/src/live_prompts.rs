//! The Settings prompt page is local-only. Use the shared prompt resolver and
//! existing persisted file; never construct a completion client for this job.
use std::path::Path;

use datazen::prompt_resolver::{PromptOverrideEntry, PromptResolver};
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen_ui_api::native_prompts::{
    NativePromptChange, NativePromptRequest, NativePromptView, PromptScenario,
};
use serde::{de::DeserializeOwned, Serialize};

pub(super) const UNAVAILABLE: &str = "Could not update local prompt settings.";

fn wire<T: DeserializeOwned>(value: impl Serialize) -> Result<T, String> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| UNAVAILABLE.into())
}

pub(super) async fn run(
    data_dir: &Path,
    registry: &DriverRegistry,
    request: NativePromptRequest,
) -> Result<NativePromptView, String> {
    let driver = if request.driver == "*" {
        None
    } else {
        if !registry.available_types().contains(&request.driver) {
            return Err(UNAVAILABLE.into());
        }
        Some(registry.get_sql_driver_by_name(&request.driver).await.ok_or(UNAVAILABLE)?)
    };
    // Registry aliases must write into the same canonical slot the resolver
    // will actually read, not a second alias-only override.
    let scope = driver.as_ref().map(|value| value.driver_type().to_string())
        .unwrap_or_else(|| "*".to_owned());
    let resolver = PromptResolver::new(data_dir, None); // production embedded defaults
    // Unlike lazy AI readiness, abort on a corrupt override file so a request
    // cannot "repair" it by silently replacing somebody's saved overrides.
    resolver.load().await.map_err(|_| UNAVAILABLE.to_owned())?;
    match request.change {
        NativePromptChange::List => {}
        NativePromptChange::Save { scenario, system } => {
            resolver.set_override(PromptOverrideEntry {
                driver_type: scope.clone(),
                scenario: wire(scenario)?,
                system,
            }).await.map_err(|_| UNAVAILABLE.to_owned())?;
        }
        NativePromptChange::Reset { scenario } => {
            let scenario = wire(scenario)?;
            let exact = resolver.get_all_overrides().await.into_iter()
                .any(|entry| entry.driver_type == scope && entry.scenario == scenario);
            if !exact {
                return Err(UNAVAILABLE.into());
            }
            resolver.remove_override(&scope, scenario).await.map_err(|_| UNAVAILABLE.to_owned())?;
        }
    }
    let resettable = resolver.get_all_overrides().await.into_iter()
        .filter(|entry| entry.driver_type == scope)
        .map(|entry| wire::<PromptScenario>(entry.scenario))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NativePromptView {
        prompts: wire(resolver.list_prompts(driver.as_deref()).await)?,
        resettable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::native_prompts::PromptSource;

    #[test]
    fn real_overrides_reopen_as_multiline_and_reset_in_the_exact_scope() {
        let dir = tempfile::tempdir().unwrap();
        let registry = super::super::host_registry();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let call = |driver: &str, change| rt.block_on(run(
            dir.path(), &registry, NativePromptRequest { driver: driver.into(), change },
        ));
        let scenario = PromptScenario::Nl2sql;
        let default = call("*", NativePromptChange::List).unwrap();
        assert_eq!(default.prompts.len(), 10);
        assert!(!default.prompts[0].system.is_empty());
        assert!(!dir.path().join("prompt_overrides.json").exists());
        let global = "custom\n第二行";
        let saved = call("*", NativePromptChange::Save { scenario, system: global.into() }).unwrap();
        assert_eq!(saved.resettable, [scenario]);
        let inherited = call("sqlite", NativePromptChange::List).unwrap();
        let p = inherited.prompts.iter().find(|p| p.scenario == scenario).unwrap();
        assert_eq!((p.source, p.system.as_str()), (PromptSource::User, global));
        assert!(inherited.resettable.is_empty());
        assert!(call("sqlite", NativePromptChange::Reset { scenario }).is_err());
        let local = call("sqlite", NativePromptChange::Save { scenario, system: "specific".into() }).unwrap();
        assert_eq!(local.resettable, [scenario]);
        let reset = call("sqlite", NativePromptChange::Reset { scenario }).unwrap();
        assert!(reset.resettable.is_empty());
        assert_eq!(reset.prompts.iter().find(|p| p.scenario == scenario).unwrap().system, global);
        let reverted = call("*", NativePromptChange::Reset { scenario }).unwrap();
        assert!(reverted.resettable.is_empty());
        assert_eq!(reverted.prompts, default.prompts);
    }

    #[test]
    fn unreadable_overrides_are_not_replaced_and_unsupported_scope_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("prompt_overrides.json");
        std::fs::write(&file, b"{broken").unwrap();
        let registry = super::super::host_registry();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        for driver in ["*", "unregistered-fixture"] {
            assert_eq!(rt.block_on(run(dir.path(), &registry, NativePromptRequest {
                driver: driver.into(),
                change: NativePromptChange::Save {
                    scenario: PromptScenario::Nl2sql, system: "new".into(),
                },
            })), Err(UNAVAILABLE.into()));
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"{broken");
    }
}
