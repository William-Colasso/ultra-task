//! Minimal Chrome DevTools Protocol client: target discovery over HTTP (`/json/list`) and
//! JSON-RPC over a plain WebSocket. Always 127.0.0.1, never TLS. Portable (std + tungstenite).
use crate::model::glob_match;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// One entry of `/json/list`.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Target {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub ws_url: String,
}

/// `Some(body)` once `buf` holds a whole HTTP response (by Content-Length, or the connection
/// closed: `eof`), `None` while more bytes are needed.
pub fn parse_http(buf: &[u8], eof: bool) -> Option<Result<String, String>> {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return eof.then(|| Err("resposta HTTP incompleta".into()));
    };
    let head = String::from_utf8_lossy(&buf[..end]);
    let body = &buf[end + 4..];
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    let len = head
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok());
    let body = match len {
        Some(n) if body.len() >= n => &body[..n],
        Some(_) if !eof => return None,
        None if !eof => return None,
        _ => body,
    };
    let text = String::from_utf8_lossy(body).into_owned();
    Some(if status == 200 { Ok(text) } else { Err(format!("HTTP {status}: {}", text.trim())) })
}

pub fn http_get(port: u16, path: &str, timeout: Duration) -> Result<String, String> {
    let mut s = TcpStream::connect_timeout(&addr(port), timeout).map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    // Chromium only answers requests whose Host is an IP or "localhost" (DNS-rebinding guard).
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let (mut buf, mut chunk) = (vec![], [0u8; 16384]);
    loop {
        let n = s.read(&mut chunk).map_err(|e| format!("127.0.0.1:{port}{path}: {e}"))?;
        buf.extend_from_slice(&chunk[..n]);
        if let Some(r) = parse_http(&buf, n == 0) {
            return r;
        }
    }
}

pub fn list_targets(port: u16) -> Result<Vec<Target>, String> {
    let body = http_get(port, "/json/list", Duration::from_secs(3))?;
    serde_json::from_str(&body).map_err(|e| format!("/json/list inválido: {e}"))
}

/// The page to drive: the first page/webview whose title matches the target rule's title
/// glob (empty or `None` = the first page). DevTools windows and workers never count.
pub fn pick_page<'a>(targets: &'a [Target], title: Option<&str>) -> Option<&'a Target> {
    let mut pages = targets
        .iter()
        .filter(|t| matches!(t.kind.as_str(), "page" | "webview") && !t.url.starts_with("devtools://") && ws_path(&t.ws_url).is_some());
    match title.map(str::trim).filter(|g| !g.is_empty()) {
        None => pages.next(),
        Some(g) => pages.find(|t| glob_match(g, &t.title)),
    }
}

/// Path part of a `ws://host:port/devtools/page/ID` URL. The host is dropped on purpose: the
/// client always reconnects to 127.0.0.1, whatever the browser advertises.
pub fn ws_path(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("ws://")?;
    rest.find('/').map(|i| &rest[i..]).filter(|p| p.len() > 1)
}

/// The reply to request `id` inside one received text frame; `None` for events and other replies.
pub fn parse_reply(text: &str, id: u64) -> Option<Result<Value, String>> {
    let v: Value = serde_json::from_str(text).ok()?;
    if v.get("id").and_then(Value::as_u64) != Some(id) {
        return None;
    }
    Some(match v.get("error") {
        Some(e) => Err(e.get("message").and_then(Value::as_str).map_or_else(|| e.to_string(), str::to_string)),
        None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
    })
}

/// `Runtime.evaluate` result → the JS value (needs `returnByValue: true`).
pub fn eval_value(result: &Value) -> Result<Value, String> {
    if let Some(ex) = result.get("exceptionDetails") {
        let msg = ex.pointer("/exception/description").or_else(|| ex.get("text")).and_then(Value::as_str).unwrap_or("exceção");
        return Err(format!("JS: {msg}"));
    }
    Ok(result.pointer("/result/value").cloned().unwrap_or(Value::Null))
}

/// One WebSocket session with a target; requests are synchronous (send, then read until the
/// matching id, skipping events).
pub struct Conn {
    ws: WebSocket<TcpStream>,
    next: u64,
    pub timeout: Duration,
}

impl Conn {
    pub fn connect(port: u16, path: &str, timeout: Duration) -> Result<Conn, String> {
        let stream = TcpStream::connect_timeout(&addr(port), timeout).map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        let _ = stream.set_nodelay(true);
        // Screenshots of big windows arrive as one large frame.
        let cfg = WebSocketConfig::default().max_message_size(Some(256 << 20)).max_frame_size(Some(256 << 20));
        let url = format!("ws://127.0.0.1:{port}{path}");
        let (ws, _) = tungstenite::client::client_with_config(url.as_str(), stream, Some(cfg)).map_err(|e| format!("WebSocket {url}: {e}"))?;
        Ok(Conn { ws, next: 0, timeout })
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let req = json!({ "id": id, "method": method, "params": params }).to_string();
        self.ws.send(Message::text(req)).map_err(|e| format!("{method}: {e}"))?;
        let end = Instant::now() + self.timeout;
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    if let Some(r) = parse_reply(t.as_str(), id) {
                        return r.map_err(|e| format!("{method}: {e}"));
                    }
                }
                Ok(Message::Close(_)) => return Err(format!("{method}: a página fechou a conexão")),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(format!("{method}: {e}")),
            }
            if Instant::now() >= end {
                return Err(format!("{method}: sem resposta em {} s", self.timeout.as_secs()));
            }
        }
    }

    pub fn eval(&mut self, expr: &str) -> Result<Value, String> {
        let r = self.call("Runtime.evaluate", json!({ "expression": expr, "returnByValue": true }))?;
        eval_value(&r)
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn t(kind: &str, title: &str, url: &str, id: &str) -> Target {
        Target { id: id.into(), kind: kind.into(), title: title.into(), url: url.into(), ws_url: format!("ws://127.0.0.1:9222/devtools/page/{id}") }
    }

    #[test]
    fn picks_page_by_title() {
        let ts = vec![
            t("service_worker", "Caixa", "https://a/sw.js", "W"),
            t("page", "DevTools", "devtools://devtools/bundled/inspector.html", "D"),
            t("page", "Nova guia", "chrome://newtab/", "N"),
            t("page", "Caixa de entrada (3)", "https://mail/", "M"),
            t("iframe", "Caixa de entrada", "https://mail/frame", "F"),
        ];
        assert_eq!(pick_page(&ts, Some("*caixa de entrada*")).unwrap().id, "M");
        assert_eq!(pick_page(&ts, None).unwrap().id, "N");
        assert_eq!(pick_page(&ts, Some("  ")).unwrap().id, "N");
        assert!(pick_page(&ts, Some("Planilha*")).is_none());
        let mut no_ws = t("page", "Sem ws", "https://x/", "X");
        no_ws.ws_url.clear(); // already attached by another DevTools client
        assert!(pick_page(&[no_ws], None).is_none());
        let wv = [t("webview", "App", "https://app/", "V")];
        assert_eq!(pick_page(&wv, Some("App")).unwrap().id, "V");
    }

    #[test]
    fn ws_paths_ignore_the_advertised_host() {
        assert_eq!(ws_path("ws://127.0.0.1:9222/devtools/page/AB"), Some("/devtools/page/AB"));
        assert_eq!(ws_path("ws://0.0.0.0:9222/devtools/browser/x"), Some("/devtools/browser/x"));
        assert_eq!(ws_path("wss://127.0.0.1:9222/devtools/page/AB"), None);
        assert_eq!(ws_path("ws://127.0.0.1:9222/"), None);
        assert_eq!(ws_path(""), None);
    }

    #[test]
    fn http_parsing() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Length:5\r\nContent-Type: application/json\r\n\r\n[1,2]";
        assert_eq!(parse_http(ok, false), Some(Ok("[1,2]".into())));
        assert_eq!(parse_http(&ok[..ok.len() - 1], false), None);
        assert_eq!(parse_http(b"HTTP/1.1 200 OK\r\n\r\n[]", false), None);
        assert_eq!(parse_http(b"HTTP/1.1 200 OK\r\n\r\n[]", true), Some(Ok("[]".into())));
        assert!(matches!(parse_http(b"HTTP/1.1 500 Internal\r\nContent-Length: 3\r\n\r\nbad", false), Some(Err(e)) if e.contains("500")));
        assert!(parse_http(b"HTTP/1.1 2", true).unwrap().is_err());
    }

    #[test]
    fn replies_and_evaluate() {
        assert_eq!(parse_reply(r#"{"method":"Page.loadEventFired","params":{}}"#, 1), None);
        assert_eq!(parse_reply(r#"{"id":2,"result":{}}"#, 1), None);
        assert_eq!(parse_reply(r#"{"id":1,"result":{"a":1}}"#, 1), Some(Ok(json!({"a":1}))));
        assert_eq!(parse_reply(r#"{"id":1,"error":{"code":-32601,"message":"'X.y' wasn't found"}}"#, 1), Some(Err("'X.y' wasn't found".into())));
        assert_eq!(eval_value(&json!({"result":{"type":"string","value":"Título"}})), Ok(json!("Título")));
        let ex = json!({"result":{"type":"object"},"exceptionDetails":{"text":"Uncaught","exception":{"description":"ReferenceError: x is not defined"}}});
        assert_eq!(eval_value(&ex), Err("JS: ReferenceError: x is not defined".into()));
    }

    /// Fake DevTools endpoint on 127.0.0.1: serves `/json/list`, then speaks JSON-RPC over a
    /// real WebSocket handshake, with an unrelated event before each reply.
    #[test]
    fn client_against_fake_server() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let (mut req, mut b) = (vec![], [0u8; 1]);
            while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap() == 1 {
                req.push(b[0]);
            }
            let req = String::from_utf8_lossy(&req).to_string();
            assert!(req.starts_with("GET /json/list HTTP/1.1\r\n"), "{req}");
            assert!(req.contains(&format!("Host: 127.0.0.1:{port}")), "{req}");
            let body = format!(r#"[{{"id":"P1","type":"page","title":"Meu App","url":"file:///a.html","webSocketDebuggerUrl":"ws://localhost:{port}/devtools/page/P1"}}]"#);
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            drop(s);

            let (s, _) = l.accept().unwrap();
            let mut ws = tungstenite::accept(s).unwrap();
            let mut seen = vec![];
            loop {
                let msg = match ws.read() {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                let v: Value = serde_json::from_str(&msg).unwrap();
                let (id, method) = (v["id"].clone(), v["method"].as_str().unwrap().to_string());
                seen.push(method.clone());
                ws.send(Message::text(r#"{"method":"Runtime.consoleAPICalled","params":{}}"#)).unwrap();
                let reply = if method == "Nao.existe" {
                    json!({ "id": id, "error": { "code": -32601, "message": "'Nao.existe' wasn't found" } })
                } else {
                    json!({ "id": id, "result": { "echo": v["params"] } })
                };
                ws.send(Message::text(reply.to_string())).unwrap();
            }
            seen
        });

        let ts = list_targets(port).unwrap();
        let page = pick_page(&ts, Some("meu*")).unwrap();
        let path = ws_path(&page.ws_url).unwrap();
        assert_eq!(path, "/devtools/page/P1");
        let mut c = Conn::connect(port, path, Duration::from_secs(5)).unwrap();
        let r = c.call("Input.dispatchKeyEvent", json!({ "type": "keyDown", "modifiers": 2 })).unwrap();
        assert_eq!(r, json!({ "echo": { "type": "keyDown", "modifiers": 2 } }));
        assert_eq!(c.call("Nao.existe", json!({})), Err("Nao.existe: 'Nao.existe' wasn't found".into()));
        assert_eq!(c.call("Input.insertText", json!({ "text": "olá" })).unwrap(), json!({ "echo": { "text": "olá" } }));
        drop(c);
        assert_eq!(srv.join().unwrap(), ["Input.dispatchKeyEvent", "Nao.existe", "Input.insertText"]);
    }
}
