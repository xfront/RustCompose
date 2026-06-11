//! 网络 — 跨平台最小 HTTP 能力，对齐 Flutter `http` / `dart:io HttpClient` 的职责。
//!
//! 设计原则（与 [`super::clipboard`] 同构）：统一 Rust API，按平台分派后端——
//!
//! - **Desktop**：零新增依赖的 `std::net::TcpStream` HTTP/1.1 实现，支持 `http://`
//!   的 GET / POST（短连接，`Connection: close`）。`https://` 需 TLS，本精简层不内建，
//!   返回 `Err(Unsupported)`，引导走宿主 channel（`httpGet` / `httpPost`）。
//! - **Web / Android / iOS / OHOS**：经 [`super::channel`] 交给宿主原生层实现
//!   （宿主用 fetch / OkHttp / URLSession 支持 HTTPS）。
//!
//! 同步接口刻意对齐 `clipboard`：网络 I/O 属阻塞操作，业务侧应在后台线程调用，
//! 结果经 [`crate::runtime::post_to_main`] 回投 GUI 线程再写 `MutableState`。

use super::error::{ErrorKind, Result, ServiceError};

/// HTTP 请求方法（与 Flutter `http` 的 `get/post` 对应）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// 一次 HTTP 响应的最小视图。
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// 发起 HTTP 请求并返回响应。
pub fn request(method: Method, url: &str, body: &str) -> Result<Response> {
    inner_request(method, url, body)
}

/// GET 简写。
pub fn get(url: &str) -> Result<Response> {
    request(Method::Get, url, "")
}

/// POST 简写。
pub fn post(url: &str, body: &str) -> Result<Response> {
    request(Method::Post, url, body)
}

fn unsupported(msg: &str) -> ServiceError {
    ServiceError { kind: ErrorKind::Unavailable, message: msg.to_string() }
}

// ── Desktop：TcpStream 最小 HTTP/1.1 ─────────────────────────────────────────
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn inner_request(method: Method, url: &str, body: &str) -> Result<Response> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let rest = url.strip_prefix("http://").ok_or_else(|| {
        unsupported("network: 精简层仅支持 http://；https:// 请经 channel 宿主实现")
    })?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(unsupported("network: 空 host"));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
            (h, p.parse::<u16>().map_err(|_| unsupported("network: 非法端口"))?)
        }
        _ => (authority, 80u16),
    };
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| unsupported(&format!("network: DNS 失败: {e}")))?
        .next()
        .ok_or_else(|| unsupported("network: 无解析地址"))?;

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
        .map_err(|e| unsupported(&format!("network: 连接失败: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|e| unsupported(&format!("network: 设置超时失败: {e}")))?;

    let verb = match method {
        Method::Get => "GET",
        Method::Post => "POST",
    };
    let req = format!(
        "{verb} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| unsupported(&format!("network: 发送失败: {e}")))?;

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| unsupported(&format!("network: 读取失败: {e}")))?;
    parse_http_response(&raw)
}

/// 解析最小 HTTP/1.1 响应（状态行 + 首部 + CRLFCRLF + 正文）。
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn parse_http_response(raw: &[u8]) -> Result<Response> {
    let sep = find_subslice(raw, b"\r\n\r\n")
        .ok_or_else(|| unsupported("network: 响应缺少首部结束符"))?;
    let head = std::str::from_utf8(&raw[..sep])
        .map_err(|_| unsupported("network: 响应首部非 UTF-8"))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| unsupported("network: 无法解析状态码"))?;
    let body = String::from_utf8_lossy(&raw[sep + 4..]).into_owned();
    Ok(Response { status, body })
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_env = "ohos"),
    not(target_os = "ios")
))]
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── Web / 移动端：经 channel 交给宿主 ─────────────────────────────────────────
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_env = "ohos",
    target_os = "ios"
))]
fn inner_request(method: Method, url: &str, body: &str) -> Result<Response> {
    use super::channel;
    let name = match method {
        Method::Get => "httpGet",
        Method::Post => "httpPost",
    };
    // arg 约定：`url`（GET）或 `url\nbody`（POST）；宿主按换行切分。
    let arg = match method {
        Method::Get => url.to_string(),
        Method::Post => format!("{url}\n{body}"),
    };
    let out = channel::invoke(name, &arg)
        .map_err(|_| unsupported("network: 宿主未实现 onMethodCall(\"httpGet/httpPost\")"))?;
    // 宿主响应约定：`STATUS\nBODY`。
    let (status, resp_body) = match out.split_once('\n') {
        Some((s, b)) => (
            s.parse::<u16>().unwrap_or(0),
            b.to_string(),
        ),
        None => (0, out),
    };
    Ok(Response { status, body: resp_body })
}
