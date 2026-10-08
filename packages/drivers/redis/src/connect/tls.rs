//! TLS material: scheme selection, certificate loading from disk, and the mode bridge.

use super::parse::non_empty;
use super::parse::scheme_for_tls;
use super::plan::TlsPlan;
use datazen_driver_api::DriverError;
use redis::cluster::TlsMode;
use redis::{ClientTlsConfig, TlsCertificates as RedisTlsCertificates};
use std::fs;
use std::path::Path;

pub(crate) fn build_node_url(
    tls: &TlsPlan,
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    db_index: Option<u32>,
) -> String {
    let scheme = scheme_for_tls(tls);
    let user = non_empty(username).map(|u| urlencoding::encode(&u).into_owned());
    let pass = password.map(|p| urlencoding::encode(p).into_owned());

    let auth = match (user.as_deref(), pass.as_deref()) {
        (Some(u), Some(p)) => format!("{u}:{p}@"),
        (None, Some(p)) => format!(":{p}@"),
        (Some(u), None) => format!("{u}@"),
        (None, None) => String::new(),
    };

    match db_index {
        Some(db) => format!("{scheme}://{auth}{host}:{port}/{db}"),
        None => format!("{scheme}://{auth}{host}:{port}"),
    }
}

pub(crate) fn tls_mode_for_plan(tls: &TlsPlan) -> Option<TlsMode> {
    if !tls.enabled {
        return None;
    }
    ensure_crypto_provider();
    if tls.insecure_skip_verify {
        Some(TlsMode::Insecure)
    } else {
        Some(TlsMode::Secure)
    }
}

fn read_pem(path: &str, label: &str) -> Result<Vec<u8>, DriverError> {
    if !Path::new(path).exists() {
        return Err(DriverError::SslError(format!(
            "TLS {label} file not found: {path}"
        )));
    }
    fs::read(path)
        .map_err(|e| DriverError::SslError(format!("failed to read TLS {label} file {path}: {e}")))
}

pub(crate) fn load_tls_certificates(
    tls: &TlsPlan,
) -> Result<Option<RedisTlsCertificates>, DriverError> {
    if !tls.enabled {
        return Ok(None);
    }
    ensure_crypto_provider();
    let client_tls = match (&tls.cert_path, &tls.key_path) {
        (Some(cert_path), Some(key_path)) => Some(ClientTlsConfig {
            client_cert: read_pem(cert_path, "client certificate")?,
            client_key: read_pem(key_path, "client private key")?,
        }),
        (None, None) => None,
        _ => {
            return Err(DriverError::SslError(
                "TLS client certificate and private key must both be set for mTLS".into(),
            ));
        }
    };
    let root_cert = tls
        .ca_path
        .as_ref()
        .map(|p| read_pem(p, "CA certificate"))
        .transpose()?;
    if client_tls.is_none() && root_cert.is_none() {
        return Ok(None);
    }
    Ok(Some(RedisTlsCertificates {
        client_tls,
        root_cert,
    }))
}

/// redis 0.27 builds rustls configs through the process default. With both
/// AWS-LC and ring enabled by different drivers, rustls cannot infer one.
/// Preserve an application-installed provider; racing installers are harmless
/// because install_default is a once-only operation.
pub(super) fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_choice_is_tested_in_fresh_processes() {
        for scenario in ["unset", "existing"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "connect::tls::tests::fresh_process_provider_case",
                    "--nocapture",
                ])
                .env("DATAZEN_TLS_PROVIDER_CASE", scenario)
                .status()
                .unwrap();
            assert!(status.success(), "provider scenario {scenario} failed");
        }
    }

    #[test]
    fn fresh_process_provider_case() {
        let Ok(scenario) = std::env::var("DATAZEN_TLS_PROVIDER_CASE") else {
            return;
        };
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
        if scenario == "existing" {
            rustls::crypto::ring::default_provider()
                .install_default()
                .unwrap();
        }
        let before = rustls::crypto::CryptoProvider::get_default().cloned();
        ensure_crypto_provider();
        let after = rustls::crypto::CryptoProvider::get_default().unwrap();
        if let Some(before) = before {
            assert!(
                std::sync::Arc::ptr_eq(&before, after),
                "must preserve the installed provider"
            );
        }
        // A builder must succeed with feature-unified ring + AWS-LC.
        let _ = rustls::ClientConfig::builder();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses",
                "--nocapture",
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }
}
