//! Build-time deployment allowlist. Never sourced from Settings, IPC, PATH or env.
use super::{DesktopNativeTools, NativeProbeInstallation};
use datazen_modules::execution::runtime::budget::InProcessBudgetCoordinator;
use datazen_modules::jobs::application::native_tool::NativeToolRuntime;
use datazen_modules::{jobs::domain::native_tool::*, shared::error::PortError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registration {
    contract: String,
    tool_id: String,
    version: String,
    executable: PathBuf,
    sha256: String,
    expected_version_line: String,
    max_processes: u32,
    max_database_sockets: u32,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeChoice {
    pub selection: NativeToolSelection,
    pub operation: &'static str,
    pub max_processes: u32,
    pub max_database_sockets: u32,
}
pub(crate) struct NativeDeployment {
    pub runtime: Arc<NativeToolRuntime>,
    port: Arc<DesktopNativeTools>,
    choices: Vec<ProbeChoice>,
    digests: HashMap<String, String>,
    #[cfg(test)]
    budget: Arc<InProcessBudgetCoordinator>,
}
fn unavailable() -> PortError {
    PortError::BackendUnavailable("native deployment unavailable or unsupported".into())
}
fn key(selection: &NativeToolSelection) -> String {
    format!("{}@{}", selection.tool_id, selection.version)
}
impl NativeDeployment {
    pub(crate) fn from_build(
        budget: Arc<InProcessBudgetCoordinator>,
    ) -> Result<Arc<Self>, PortError> {
        let manifest = include_str!("../../../resources/native-tool-probes.json");
        Self::from_manifest(manifest, budget)
    }
    fn from_manifest(
        manifest: &str,
        budget: Arc<InProcessBudgetCoordinator>,
    ) -> Result<Arc<Self>, PortError> {
        if manifest.len() > 65_536 {
            return Err(unavailable());
        }
        let registrations: Vec<Registration> =
            serde_json::from_str(manifest).map_err(|_| unavailable())?;
        if registrations.len() > 32 {
            return Err(unavailable());
        }
        let mut installations = Vec::new();
        let mut choices = Vec::new();
        let mut digests = HashMap::new();
        for entry in registrations {
            // Only the reviewed --version contract exists. Nonzero DB cost is
            // unsupported, NOT coerced to zero or treated as a dump/restore.
            if entry.contract != "versionProbeV1"
                || entry.max_processes != 1
                || entry.max_database_sockets != 0
                || entry.version.is_empty()
                || entry.version.len() > 128
                || entry.version.chars().any(char::is_control)
            {
                return Err(unavailable());
            }
            let sha256: [u8; 32] = decode_digest(&entry.sha256)?;
            let selection = NativeToolSelection {
                tool_id: entry.tool_id.clone(),
                version: entry.version.clone(),
            };
            let digest = format!(
                "sha256:{:x}",
                Sha256::digest(
                    serde_json::to_vec(&(
                        &entry.contract,
                        &selection,
                        &entry.sha256,
                        &entry.expected_version_line,
                        entry.max_processes,
                        entry.max_database_sockets
                    ))
                    .map_err(|_| unavailable())?
                )
            );
            if digests.insert(key(&selection), digest).is_some() {
                return Err(unavailable());
            }
            choices.push(ProbeChoice {
                selection,
                operation: "versionProbe",
                max_processes: 1,
                max_database_sockets: 0,
            });
            installations.push(NativeProbeInstallation {
                tool_id: entry.tool_id,
                version: entry.version,
                executable: entry.executable,
                sha256,
                expected_version_line: entry.expected_version_line,
            });
        }
        let port = Arc::new(DesktopNativeTools::new(installations)?);
        let runtime =
            NativeToolRuntime::new(port.clone(), budget.clone(), 1, NativeToolLimits::default())?;
        Ok(Arc::new(Self {
            runtime,
            port,
            choices,
            digests,
            #[cfg(test)]
            budget,
        }))
    }
    #[cfg(test)]
    pub(crate) fn fixture_manifest(&self, manifest: &str) -> Result<Arc<Self>, PortError> {
        Self::from_manifest(manifest, self.budget.clone())
    }
    pub(crate) fn choices(&self) -> Vec<ProbeChoice> {
        self.choices
            .iter()
            .filter(|choice| self.port.resolve(&choice.selection).is_ok())
            .cloned()
            .collect()
    }
    pub(crate) fn freeze(&self, selection: &NativeToolSelection) -> Result<String, PortError> {
        self.port.resolve(selection)?; // Revalidate binary and full declared bound before accept.
        self.digests
            .get(&key(selection))
            .cloned()
            .ok_or_else(unavailable)
    }
}
fn decode_digest(raw: &str) -> Result<[u8; 32], PortError> {
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(unavailable());
    }
    let mut result = [0u8; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16).map_err(|_| unavailable())?;
    }
    Ok(result)
}
