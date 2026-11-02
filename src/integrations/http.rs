//! Destino REST. O transporte é o WinHTTP do próprio Windows: usa o repositório de
//! certificados e o proxy do sistema, e não traz dependência nova nem toolchain C.
use super::{Body, Destination, Job, Outcome, check_interp};
use crate::model::interpolate;
use serde::{Deserialize, Serialize};
use windows::Win32::Networking::WinHttp::*;
use windows::core::PCWSTR;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub const ALL: [Method; 5] = [Method::Get, Method::Post, Method::Put, Method::Patch, Method::Delete];
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct HttpDest {
    pub method: Method,
    /// Aceita `{expressão}`.
    pub url: String,
    /// Pares `["Nome", "valor"]`; o valor aceita `{expressão}`.
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
}

impl Default for HttpDest {
    fn default() -> Self {
        HttpDest { method: Method::Get, url: "https://".into(), headers: vec![], timeout_ms: 10000 }
    }
}

/// Uma requisição pronta, sem nenhuma expressão pendente.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    /// `None` em GET: o corpo não vai.
    pub body: Option<String>,
    pub timeout_ms: u64,
}

impl Destination for HttpDest {
    fn kind(&self) -> &'static str {
        "Http"
    }

    fn validate(&self, path: &str, errs: &mut Vec<String>) {
        let lower = self.url.trim().to_lowercase();
        if lower.is_empty() {
            errs.push(format!("{path}.url: vazia"));
        } else if !lower.starts_with('{') && !lower.starts_with("http://") && !lower.starts_with("https://") {
            errs.push(format!("{path}.url: deve começar com http:// ou https://"));
        }
        check_interp(errs, &format!("{path}.url"), &self.url);
        for (k, (n, v)) in self.headers.iter().enumerate() {
            if n.trim().is_empty() || n.contains([':', '\r', '\n']) {
                errs.push(format!("{path}.headers[{k}]: nome de cabeçalho inválido \"{n}\""));
            }
            if v.contains(['\r', '\n']) {
                errs.push(format!("{path}.headers[{k}]: o valor não pode ter quebra de linha"));
            }
            check_interp(errs, &format!("{path}.headers[{k}]"), v);
        }
        if self.timeout_ms == 0 {
            errs.push(format!("{path}.timeout_ms: deve ser > 0"));
        }
    }

    fn prepare(&self, body: Body, eval: &mut dyn FnMut(&str) -> Result<String, String>) -> Result<Job, String> {
        let req = self.request(body, eval)?;
        Ok(Box::new(move || send(&req)))
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

impl HttpDest {
    pub fn request(&self, body: Body, eval: &mut dyn FnMut(&str) -> Result<String, String>) -> Result<Request, String> {
        let url = parse_url(&interpolate(&self.url, &mut *eval)?)?;
        let mut headers = vec![];
        for (n, v) in &self.headers {
            headers.push((n.trim().to_string(), interpolate(v, &mut *eval)?));
        }
        let send_body = self.method != Method::Get;
        if send_body && !headers.iter().any(|h| h.0.eq_ignore_ascii_case("content-type")) {
            headers.push(("Content-Type".into(), body.content_type.into()));
        }
        Ok(Request { method: self.method, url, headers, body: send_body.then_some(body.text), timeout_ms: self.timeout_ms })
    }
}

// ---------- URL ----------

#[derive(Clone, Debug, PartialEq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    /// Caminho + consulta, já com os caracteres inválidos em %XX.
    pub path: String,
}

pub fn parse_url(s: &str) -> Result<Url, String> {
    let s = s.trim();
    let bad = || format!("URL inválida \"{s}\"");
    let (https, rest) = match s.split_once("://") {
        Some((sch, rest)) if sch.eq_ignore_ascii_case("https") => (true, rest),
        Some((sch, rest)) if sch.eq_ignore_ascii_case("http") => (false, rest),
        _ => return Err(format!("{}: deve começar com http:// ou https://", bad())),
    };
    let rest = rest.split('#').next().unwrap_or("");
    let cut = rest.find(['/', '?']).unwrap_or(rest.len());
    let (auth, path) = rest.split_at(cut);
    if auth.contains('@') {
        return Err(format!("{}: usuário/senha na URL não são aceitos; use um cabeçalho Authorization", bad()));
    }
    let (host, port) = if let Some(v6) = auth.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or_else(bad)?;
        (h.to_string(), after.strip_prefix(':'))
    } else {
        match auth.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (auth.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(format!("{}: falta o servidor", bad()));
    }
    let port = match port {
        Some(p) => p.parse().map_err(|_| format!("{}: porta \"{p}\"", bad()))?,
        None if https => 443,
        None => 80,
    };
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    Ok(Url { https, host, port, path: escape_path(&path) })
}

/// Troca por %XX o que não pode ir cru numa URL (espaço, acentos, aspas…); mantém `/ ? & = %`.
fn escape_path(p: &str) -> String {
    let mut out = String::new();
    for b in p.bytes() {
        if b.is_ascii_graphic() && !b"\"<>\\^`{|}".contains(&b) {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.starts_with("127.") || host == "::1"
}

// ---------- WinHTTP ----------

struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn describe(e: &windows::core::Error) -> String {
    let code = (e.code().0 as u32) & 0xFFFF;
    let what = match code {
        12002 => "tempo esgotado",
        12007 => "servidor não encontrado (DNS)",
        12029 => "não conectou ao servidor",
        12030 | 12031 => "a conexão caiu",
        12152 => "resposta inválida do servidor",
        12037 | 12038 | 12044 | 12045 | 12169 | 12175 | 12179 => "falha de certificado/TLS",
        _ => return format!("erro WinHTTP {code}: {}", e.message()),
    };
    format!("{what} (WinHTTP {code})")
}

fn check(h: *mut core::ffi::c_void) -> Result<Handle, String> {
    if h.is_null() { Err(describe(&windows::core::Error::from_thread())) } else { Ok(Handle(h)) }
}

/// Faz a requisição de forma síncrona (roda na thread da integração).
pub fn send(req: &Request) -> Outcome {
    match unsafe { send_raw(req) } {
        Ok((status, body)) => Outcome { ok: (200..300).contains(&status), status: status as i64, body: String::from_utf8_lossy(&body).into_owned(), error: None },
        Err(e) => Outcome::failed(e),
    }
}

unsafe fn send_raw(req: &Request) -> Result<(u32, Vec<u8>), String> {
    unsafe {
        let agent = wide("UltraTask");
        let u = &req.url;
        // Loopback vai direto; o resto segue o proxy do sistema (WPAD/PAC quando houver).
        let session = if is_loopback(&u.host) {
            check(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_NO_PROXY, PCWSTR::null(), PCWSTR::null(), 0))?
        } else {
            check(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0))
                .or_else(|_| check(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0)))?
        };
        let t = req.timeout_ms.min(i32::MAX as u64) as i32;
        WinHttpSetTimeouts(session.0, t, t, t, t).map_err(|e| describe(&e))?;
        let host = wide(&u.host);
        let conn = check(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), u.port, 0))?;
        let (verb, path) = (wide(req.method.as_str()), wide(&u.path));
        let flags = if u.https { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let r = check(WinHttpOpenRequest(conn.0, PCWSTR(verb.as_ptr()), PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags))?;
        let hdrs: Vec<u16> = req.headers.iter().map(|(n, v)| format!("{n}: {v}\r\n")).collect::<String>().encode_utf16().collect();
        let body = req.body.as_deref().unwrap_or("").as_bytes();
        let len = body.len() as u32;
        WinHttpSendRequest(r.0, (!hdrs.is_empty()).then_some(&hdrs[..]), (len > 0).then_some(body.as_ptr() as *const _), len, len, 0).map_err(|e| describe(&e))?;
        WinHttpReceiveResponse(r.0, std::ptr::null_mut()).map_err(|e| describe(&e))?;
        let (mut status, mut size) = (0u32, 4u32);
        WinHttpQueryHeaders(r.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as *mut _), &mut size, std::ptr::null_mut()).map_err(|e| describe(&e))?;
        let mut out = vec![];
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(r.0, &mut avail).map_err(|e| describe(&e))?;
            if avail == 0 {
                break;
            }
            let start = out.len();
            out.resize(start + avail as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(r.0, out[start..].as_mut_ptr() as *mut _, avail, &mut read).map_err(|e| describe(&e))?;
            out.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok((status, out))
    }
}
