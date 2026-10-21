use datazen_ai_providers::{
    custom::{fetch_remote_models_with_client, CustomProtocol},
    http::HttpClient,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

#[test]
fn desktop_proxy_resolves_target() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut buffer = [0; 4096];
                    let length = socket.read(&mut buffer).unwrap();
                    assert!(String::from_utf8_lossy(&buffer[..length])
                        .contains("proxy-only-target.invalid"));
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                    return;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("proxy was never reached: {error}"),
            }
        }
    });
    // A subprocess isolates proxy environment changes from concurrently running tests.
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "desktop_proxy_child"])
        .env("DATAZEN_EGRESS_PROXY_CHILD", "1")
        .env("http_proxy", &proxy)
        .env("HTTP_PROXY", &proxy)
        .env("all_proxy", &proxy)
        .env("ALL_PROXY", &proxy)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .status()
        .unwrap();
    server.join().unwrap();
    assert!(result.success());
}

#[test]
fn desktop_proxy_child() {
    if std::env::var("DATAZEN_EGRESS_PROXY_CHILD").as_deref() != Ok("1") {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let response = HttpClient::desktop()
            .get("http://proxy-only-target.invalid/v1/models")
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    });
}

#[tokio::test]
async fn model_listing_keeps_thirty_second_total_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (_socket, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_secs(32));
    });
    let result = tokio::time::timeout(
        Duration::from_secs(31),
        fetch_remote_models_with_client(
            HttpClient::desktop(),
            CustomProtocol::OpenAiCompatible,
            &endpoint,
            "synthetic",
        ),
    )
    .await;
    server.join().unwrap();
    assert!(
        result.is_ok(),
        "model listing exceeded former thirty second deadline"
    );
    assert!(
        result.unwrap().is_err(),
        "a server that never responds must time out"
    );
}
