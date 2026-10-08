//! A genuine Redis TLS handshake in a fresh process with both crypto providers.
use super::plan::TlsPlan;
use super::standalone::open_standalone_conn_with_fallback;
use std::time::Duration;

#[test]
fn tls_handshake_runs_in_a_fresh_process() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "connect::tls_journey::fresh_tls_handshake",
            "--nocapture",
        ])
        .env("DATAZEN_TLS_HANDSHAKE_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn fresh_tls_handshake() {
    if std::env::var_os("DATAZEN_TLS_HANDSHAKE_CHILD").is_none() {
        return;
    }
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let Some(server) = crate::live_server::LiveRedis::start_tls().unwrap() else {
        eprintln!("SKIP Redis TLS: redis-server or openssl unavailable");
        return;
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::net::TcpStream::connect(("127.0.0.1", server.port()))
        .await
        .is_err()
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "TLS Redis did not start"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let tls = TlsPlan {
        enabled: true,
        prefer_fallback: false,
        ca_path: Some(server.ca_path().to_str().unwrap().to_string()),
        cert_path: None,
        key_path: None,
        key_passphrase: None,
        insecure_skip_verify: false,
    };
    let url = format!("rediss://127.0.0.1:{}/0", server.port());
    let mut conn =
        open_standalone_conn_with_fallback(&url, &tls, None, None, Duration::from_secs(3))
            .await
            .unwrap();
    let pong: String = redis::cmd("PING").query_async(&mut conn).await.unwrap();
    assert_eq!(pong, "PONG");
    assert!(rustls::crypto::CryptoProvider::get_default().is_some());
}
