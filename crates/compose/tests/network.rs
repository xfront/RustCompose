//! network 平台服务回归：桌面 TcpStream HTTP/1.1 的最小实现，用本地 TcpListener 起服务，
//! 零外网依赖。非桌面目标经 channel 交给宿主，本测试文件在这些目标下为空。

#![cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use compose::platform::services::network::{self, Response};

/// 起一个只接受一次连接、回固定响应的迷你 HTTP/1.1 服务，返回 (url, 收到请求体的通道)。
fn serve_once(status_line: &str, body: &str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let status = status_line.to_string();
    let payload = body.to_string();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        let _ = stream.write_all(resp.as_bytes());
    });
    (format!("http://{addr}/probe"), rx)
}

#[test]
fn get_parses_status_and_body() {
    let (url, _rx) = serve_once("200 OK", "pong");
    let resp = network::get(&url).expect("GET 应成功");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, "pong");
    assert!(resp.ok());
}

#[test]
fn post_sends_body_and_reads_response() {
    let (url, rx) = serve_once("201 Created", "created");
    let resp = network::post(&url, "hello=world").expect("POST 应成功");
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body, "created");
    let req = rx.recv().expect("服务应收到请求");
    assert!(req.starts_with("POST /probe HTTP/1.1\r\n"), "请求行错误: {req}");
    assert!(req.contains("Content-Length: 11"), "缺 Content-Length: {req}");
    assert!(req.ends_with("hello=world"), "正文未送达: {req}");
}

#[test]
fn non_2xx_is_not_ok() {
    let (url, _rx) = serve_once("404 Not Found", "missing");
    let resp: Response = network::get(&url).expect("404 也是合法响应");
    assert_eq!(resp.status, 404);
    assert!(!resp.ok());
}

#[test]
fn https_is_rejected_with_unavailable() {
    let err = network::get("https://example.com/").expect_err("精简层不支持 https");
    assert_eq!(err.kind, compose::platform::services::error::ErrorKind::Unavailable);
    assert!(err.message.contains("http://"), "应提示改用 channel: {}", err.message);
}

#[test]
fn channel_host_bridge_is_used_when_registered() {
    // 桌面路径不走 channel；本用例验证 invoke 约定在宿主注册后能被网络解析逻辑消费。
    // 通过 register_static 注册 httpGet 的「STATUS\nBODY」响应，确认通道原语可用。
    compose::platform::services::channel::register_static(&[("httpGet", "200\nfrom-host")]);
    let raw = compose::platform::services::channel::invoke("httpGet", "http://x/").expect("channel 应命中");
    let (status, body) = raw.split_once('\n').unwrap();
    assert_eq!(status.parse::<u16>().unwrap(), 200);
    assert_eq!(body, "from-host");
    // 注意：桌面 inner_request 仍走 TcpStream；此处只验证宿主桥接的数据约定。
}
