use super::*;
use evalexpr::ContextWithMutableFunctions;
use crate::model::{Block, CondKind, Condition, Ev, OnTimeout, Region, Script};
use crate::runner::{Debug, Halt, Interp, Io};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------- contrato do JSON, validação e lógica pura ----------

const TICKET_JSON: &str = r#"{
  "name": "cep",
  "integrations": [{
    "name": "ViaCEP",
    "when": { "OnSet": ["cep"] },
    "dest": { "Http": { "method": "GET", "url": "https://viacep.com.br/ws/{cep}/json/", "headers": [], "timeout_ms": 10000 } },
    "payload": { "Json": ["cep"] },
    "result": "endereco"
  }, {
    "name": "batida",
    "when": { "Every": { "s": 5.0 } },
    "dest": { "Http": { "method": "POST", "url": "http://exemplo/x", "headers": [["Authorization", "Bearer {token}"]], "timeout_ms": 3000 } },
    "payload": { "Csv": ["a", "b"] },
    "result": null
  }, {
    "name": "fim",
    "when": "OnEnd",
    "dest": { "Http": { "method": "PATCH", "url": "http://exemplo/fim", "headers": [], "timeout_ms": 3000 } },
    "payload": { "Text": "pedido {n}" },
    "result": null
  }]
}"#;

#[test]
fn json_contract() {
    let s: Script = serde_json::from_str(TICKET_JSON).unwrap();
    assert_eq!(s.integrations.len(), 3);
    let v = &s.integrations[0];
    assert_eq!(v.when, When::OnSet(vec!["cep".into()]));
    assert_eq!(v.dest, Dest::Http(HttpDest { method: Method::Get, url: "https://viacep.com.br/ws/{cep}/json/".into(), headers: vec![], timeout_ms: 10000 }));
    assert_eq!(v.payload, Payload::Json(vec!["cep".into()]));
    assert_eq!(v.result.as_deref(), Some("endereco"));
    assert_eq!(s.integrations[1].when, When::Every { s: 5.0 });
    assert_eq!(s.integrations[1].payload, Payload::Csv(vec!["a".into(), "b".into()]));
    assert_eq!(s.integrations[2].when, When::OnEnd);
    assert_eq!(s.integrations[2].result, None);
    // Ida e volta, e o formato serializado é o do contrato.
    let txt = serde_json::to_string(&s).unwrap();
    assert!(txt.contains(r#""when":{"OnSet":["cep"]}"#), "{txt}");
    assert!(txt.contains(r#""when":"OnEnd""#), "{txt}");
    assert!(txt.contains(r#""method":"GET""#), "{txt}");
    assert_eq!(serde_json::from_str::<Script>(&txt).unwrap(), s);
    // Scripts antigos (sem integrações) continuam válidos e não ganham o campo ao salvar.
    let old: Script = serde_json::from_str(r#"{"name":"velho","blocks":["Stop"]}"#).unwrap();
    assert!(old.integrations.is_empty());
    assert!(!serde_json::to_string(&old).unwrap().contains("integrations"));
    assert_eq!(crate::model::validate(&s, &[]), Vec::<String>::new());
}

#[test]
fn rejects_unknown_fields() {
    let bad = [
        r#"{"name":"x","integrations":[{"name":"a","quando":"OnEnd"}]}"#,
        r#"{"name":"x","integrations":[{"name":"a","when":{"Every":{"s":1.0,"ms":3}}}]}"#,
        r#"{"name":"x","integrations":[{"name":"a","dest":{"Http":{"url":"http://a","verbo":"GET"}}}]}"#,
        r#"{"name":"x","integrations":[{"name":"a","dest":{"Http":{"method":"OPTIONS"}}}]}"#,
        r#"{"name":"x","integrations":[{"name":"a","payload":{"Xml":["a"]}}]}"#,
    ];
    for b in bad {
        assert!(serde_json::from_str::<Script>(b).is_err(), "aceitou {b}");
    }
    // Campos omitidos usam o padrão.
    let s: Script = serde_json::from_str(r#"{"name":"x","integrations":[{"name":"a","when":"OnEnd","dest":{"Http":{"url":"http://a/b"}}}]}"#).unwrap();
    let Dest::Http(h) = &s.integrations[0].dest else { panic!() };
    assert_eq!((h.method, h.timeout_ms, s.integrations[0].result.clone()), (Method::Get, 10000, None));
}

#[test]
fn validator() {
    let http = |url: &str, headers: Vec<(String, String)>| Dest::Http(HttpDest { url: url.into(), headers, ..Default::default() });
    let s = Script {
        name: "v".into(),
        integrations: vec![
            Integration { name: "ok".into(), when: When::OnSet(vec!["cep".into()]), dest: http("http://h/{cep}", vec![]), payload: Payload::Json(vec!["cep".into()]), result: Some("r".into()) },
            Integration { name: "".into(), when: When::OnSet(vec!["1a".into(), "r2".into()]), dest: http("ftp://h", vec![]), payload: Payload::Csv(vec!["b c".into()]), result: Some("r2".into()) },
            Integration { name: "c".into(), when: When::Every { s: 0.0 }, dest: http("http://h/{x +}", vec![("".into(), "{y >}".into())]), payload: Payload::Text("oi {a +}".into()), result: Some("9r".into()) },
            Integration { name: "d".into(), when: When::OnSet(vec!["r3_pendente".into()]), dest: http("{base}/x", vec![]), payload: Payload::Json(vec![]), result: Some("r3".into()) },
        ],
        ..Default::default()
    };
    let errs = crate::model::validate(&s, &[]);
    let paths: Vec<&str> = errs.iter().map(|e| e.split(": ").next().unwrap()).collect();
    assert_eq!(
        paths,
        [
            "integrations[1].name",
            "integrations[1].when.OnSet[0]",
            "integrations[1].when.OnSet",
            "integrations[1].payload.Csv[0]",
            "integrations[1].dest.Http.url",
            "integrations[2].result",
            "integrations[2].when.Every.s",
            "integrations[2].payload.Text",
            "integrations[2].dest.Http.url",
            "integrations[2].dest.Http.headers[0]",
            "integrations[2].dest.Http.headers[0]",
            "integrations[3].when.OnSet",
        ],
        "{errs:#?}"
    );
    assert!(errs[2].contains("dispararia a si mesma"), "{}", errs[2]);
    assert!(errs[11].contains("r3_pendente"), "{}", errs[11]);
}

pub(super) fn ctx(pairs: &[(&str, Value)]) -> HashMapContext {
    use evalexpr::ContextWithMutableVariables;
    let mut c = HashMapContext::new();
    for (k, v) in pairs {
        c.set_value((*k).into(), v.clone()).unwrap();
    }
    c
}

#[test]
fn payload_formats() {
    let vars = ctx(&[("n", Value::Int(5)), ("f", Value::Float(2.5)), ("s", Value::String("a,\"b\"".into())), ("b", Value::Boolean(true))]);
    let j = Payload::Json(vec!["n".into(), "f".into(), "s".into(), "b".into()]).render(&vars).unwrap();
    assert_eq!(j.text, r#"{"n":5,"f":2.5,"s":"a,\"b\"","b":true}"#);
    assert!(j.content_type.starts_with("application/json"));
    let c = Payload::Csv(vec!["n".into(), "s".into(), "b".into(), "f".into()]).render(&vars).unwrap();
    assert_eq!(c.text, r#"5,"a,""b""",true,2.5"#);
    assert!(c.content_type.starts_with("text/csv"));
    let t = Payload::Text("pedido {n + 1}: {s}".into()).render(&vars).unwrap();
    assert_eq!(t.text, "pedido 6: a,\"b\"");
    assert!(t.content_type.starts_with("text/plain"));
    assert!(Payload::Json(vec!["nada".into()]).render(&vars).unwrap_err().contains("nada"));
}

#[test]
fn json_function() {
    let doc = r#"{"logradouro":"Praça da Sé","n":12,"p":2.5,"ok":true,"z":null,"itens":[{"preco":9.9},{"preco":3}]}"#;
    assert_eq!(json_get(doc, "/logradouro"), Ok(Value::String("Praça da Sé".into())));
    assert_eq!(json_get(doc, "/n"), Ok(Value::Int(12)));
    assert_eq!(json_get(doc, "/p"), Ok(Value::Float(2.5)));
    assert_eq!(json_get(doc, "/ok"), Ok(Value::Boolean(true)));
    assert_eq!(json_get(doc, "/itens/1/preco"), Ok(Value::Int(3)));
    assert_eq!(json_get(doc, "itens/0/preco"), Ok(Value::Float(9.9))); // sem a barra inicial também vale
    assert_eq!(json_get(doc, "/z"), Ok(Value::String(String::new())));
    assert_eq!(json_get(doc, "/naoexiste"), Ok(Value::String(String::new())));
    assert_eq!(json_get(doc, "/itens/0"), Ok(Value::String(r#"{"preco":9.9}"#.into())));
    assert!(json_get("isso não é json", "/a").is_err());
    assert_eq!(json_get("", "/a"), Ok(Value::String(String::new()))); // resposta que não chegou
    // Pela expressão, como o fluxo usa.
    let mut c = ctx(&[("r", Value::String(doc.into()))]);
    for (name, f) in crate::runner::custom_functions() {
        let _ = c.set_function(name.into(), f);
    }
    assert_eq!(evalexpr::eval_with_context(r#"json(r, "/itens/0/preco") * 2"#, &c), Ok(Value::Float(19.8)));
    assert_eq!(evalexpr::eval_with_context(r#"json(r, "/logradouro") == "Praça da Sé""#, &c), Ok(Value::Boolean(true)));
    assert!(evalexpr::eval_with_context(r#"json("{", "/a")"#, &c).is_err());
}

#[test]
fn url_and_request() {
    let u = http::parse_url("https://viacep.com.br/ws/01001000/json/").unwrap();
    assert_eq!((u.https, u.host.as_str(), u.port, u.path.as_str()), (true, "viacep.com.br", 443, "/ws/01001000/json/"));
    let u = http::parse_url("http://127.0.0.1:8080?q=a b&x=ç#frag").unwrap();
    assert_eq!((u.https, u.host.as_str(), u.port, u.path.as_str()), (false, "127.0.0.1", 8080, "/?q=a%20b&x=%C3%A7"));
    let u = http::parse_url("HTTP://[::1]:9/x").unwrap();
    assert_eq!((u.host.as_str(), u.port), ("::1", 9));
    for bad in ["ftp://a", "http://", "http://a:porta/", "http://u:s@a/"] {
        assert!(http::parse_url(bad).is_err(), "{bad}");
    }

    let vars = ctx(&[("cep", Value::String("01001000".into())), ("token", Value::String("abc".into()))]);
    let body = Payload::Json(vec!["cep".into()]).render(&vars).unwrap();
    let mut eval = |e: &str| eval_text(&vars, e);
    let get = HttpDest { url: "https://viacep.com.br/ws/{cep}/json/".into(), headers: vec![("Authorization".into(), "Bearer {token}".into())], ..Default::default() };
    let r = get.request(body.clone(), &mut eval).unwrap();
    assert_eq!(r.url.path, "/ws/01001000/json/");
    assert_eq!(r.headers, [("Authorization".to_string(), "Bearer abc".to_string())]);
    assert_eq!(r.body, None); // GET não envia corpo
    let post = HttpDest { method: Method::Post, url: "http://h/x".into(), ..Default::default() };
    let r = post.request(body.clone(), &mut eval).unwrap();
    assert_eq!(r.body.as_deref(), Some(r#"{"cep":"01001000"}"#));
    assert_eq!(r.headers, [("Content-Type".to_string(), "application/json; charset=utf-8".to_string())]);
    let own = HttpDest { method: Method::Post, url: "http://h/x".into(), headers: vec![("content-type".into(), "application/vnd.x+json".into())], ..Default::default() };
    assert_eq!(own.request(body, &mut eval).unwrap().headers.len(), 1);
}

/// A aba desenha todas as variantes sem pânico e sem mexer no que não foi editado.
#[test]
fn editor_renders_every_variant() {
    let mut s: Script = serde_json::from_str(TICKET_JSON).unwrap();
    s.integrations.push(Integration::default());
    let before = s.integrations.clone();
    let ctx = eframe::egui::Context::default();
    for _ in 0..2 {
        ctx.run_ui(Default::default(), |ui| editor::integrations_ui(ui, &mut s.integrations)).textures_delta.clear();
    }
    assert_eq!(s.integrations, before);
}

/// Sem rede: as variáveis de resultado existem desde o início, e um disparo que falha antes de
/// enviar (variável inexistente) volta na hora com `_ok = false` e vai para o log.
#[test]
fn result_vars_and_fire_errors_without_network() {
    let list = vec![Integration {
        name: "quebrada".into(),
        when: When::OnSet(vec!["x".into()]),
        dest: Dest::Http(HttpDest { url: "http://127.0.0.1:9/{nao_existe}".into(), ..Default::default() }),
        payload: Payload::Json(vec![]),
        result: Some("r".into()),
    }];
    let mut io = FakeIo::default();
    let stop = AtomicBool::new(false);
    let dbg = Debug::default();
    let mut logs = vec![];
    {
        let mut log = |m: String| logs.push(m);
        let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut log, &dbg);
        it.start_integrations(&list);
        let names: Vec<String> = dbg.vars.lock().unwrap().iter().map(|v| v.0.clone()).collect();
        assert_eq!(names, ["r", "r_ok", "r_pendente", "r_status"]);
        let b = vec![set("x", "1"), Block::If { cond: expr("!r_pendente && !r_ok && r_status == 0 && r == \"\""), then: vec![ty("falhou")], else_: vec![] }];
        assert_eq!(it.run_blocks(&b), Ok(()));
    }
    assert_eq!(io.typed, ["falhou"]);
    assert!(logs.iter().any(|l| l.contains("integração 'quebrada'") && l.contains("nao_existe")), "{logs:?}");
}

// ---------- ponta a ponta com um servidor HTTP local ----------

#[derive(Default)]
struct FakeIo {
    t: u64,
    typed: Vec<String>,
}

impl Io for FakeIo {
    fn send(&mut self, _: &Ev) -> Result<(), Halt> {
        Ok(())
    }
    fn type_text(&mut self, s: &str) -> Result<(), Halt> {
        self.typed.push(s.into());
        Ok(())
    }
    fn pixel(&mut self, _: i32, _: i32) -> Result<[u8; 3], Halt> {
        Ok([0; 3])
    }
    fn ocr(&mut self, _: Region) -> Result<String, Halt> {
        Ok(String::new())
    }
    fn window_exists(&mut self) -> bool {
        true
    }
    fn window_foreground(&mut self) -> bool {
        true
    }
    fn window_title(&mut self) -> String {
        String::new()
    }
    fn now_ms(&mut self) -> u64 {
        self.t
    }
    fn sleep(&mut self, ms: u64) {
        self.t += ms;
    }
}

/// Relógio real: as respostas HTTP chegam em tempo real, então as esperas também são reais.
pub(super) struct ClockIo {
    t0: Instant,
    typed: Vec<(u64, String)>,
}

impl ClockIo {
    fn new() -> Self {
        ClockIo { t0: Instant::now(), typed: vec![] }
    }
    fn texts(&self) -> Vec<&str> {
        self.typed.iter().map(|t| t.1.as_str()).collect()
    }
}

impl Io for ClockIo {
    fn send(&mut self, _: &Ev) -> Result<(), Halt> {
        Ok(())
    }
    fn type_text(&mut self, s: &str) -> Result<(), Halt> {
        let t = self.now_ms();
        self.typed.push((t, s.into()));
        Ok(())
    }
    fn pixel(&mut self, _: i32, _: i32) -> Result<[u8; 3], Halt> {
        Ok([0; 3])
    }
    fn ocr(&mut self, _: Region) -> Result<String, Halt> {
        Ok(String::new())
    }
    fn window_exists(&mut self) -> bool {
        true
    }
    fn window_foreground(&mut self) -> bool {
        true
    }
    fn window_title(&mut self) -> String {
        String::new()
    }
    fn now_ms(&mut self) -> u64 {
        self.t0.elapsed().as_millis() as u64
    }
    fn sleep(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

#[derive(Clone, Debug)]
struct Req {
    method: String,
    path: String,
    content_type: String,
    body: String,
}

/// Servidor HTTP/1.1 mínimo, uma thread por conexão. Rotas:
/// `/ws/01001000/json/` (ViaCEP, 300 ms), `/ws/…` (CEP desconhecido), `/lento` (2 s), `/404`, e o resto ecoa o corpo.
struct Server {
    port: u16,
    reqs: Arc<Mutex<Vec<Req>>>,
}

impl Server {
    fn start() -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let reqs = Arc::new(Mutex::new(vec![]));
        let r2 = reqs.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let r = r2.clone();
                std::thread::spawn(move || Self::handle(s, r));
            }
        });
        Server { port, reqs }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn reqs(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }

    fn handle(s: TcpStream, reqs: Arc<Mutex<Vec<Req>>>) {
        let mut rd = BufReader::new(s.try_clone().unwrap());
        let mut line = String::new();
        if rd.read_line(&mut line).is_err() {
            return;
        }
        let mut parts = line.split_whitespace();
        let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
        let (mut len, mut content_type) = (0usize, String::new());
        loop {
            let mut h = String::new();
            if rd.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                break;
            }
            if let Some((n, v)) = h.split_once(':') {
                match n.trim().to_ascii_lowercase().as_str() {
                    "content-length" => len = v.trim().parse().unwrap_or(0),
                    "content-type" => content_type = v.trim().to_string(),
                    _ => {}
                }
            }
        }
        let mut body = vec![0; len];
        let _ = rd.read_exact(&mut body);
        let body = String::from_utf8_lossy(&body).into_owned();
        reqs.lock().unwrap().push(Req { method, path: path.clone(), content_type, body: body.clone() });
        let (code, out) = if path == "/ws/01001000/json/" {
            std::thread::sleep(Duration::from_millis(300));
            (200, r#"{"cep":"01001-000","logradouro":"Praça da Sé","bairro":"Sé"}"#.to_string())
        } else if path.starts_with("/ws/") {
            (200, r#"{"erro":true}"#.to_string())
        } else if path == "/lento" {
            std::thread::sleep(Duration::from_millis(2000));
            (200, "ok lento".to_string())
        } else if path == "/404" {
            (404, "nao achei".to_string())
        } else {
            (200, body)
        };
        let mut s = rd.into_inner();
        let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
        let _ = s.flush();
    }
}

pub(super) fn set(n: &str, e: &str) -> Block {
    Block::SetVar { name: n.into(), expr: e.into() }
}
pub(super) fn ty(t: &str) -> Block {
    Block::Type { text: t.into() }
}
pub(super) fn expr(e: &str) -> Condition {
    Condition { kind: CondKind::Expr(e.into()), negate: false }
}
pub(super) fn wait_until(e: &str, ms: u32) -> Block {
    Block::WaitUntil { cond: expr(e), timeout_ms: ms.to_string(), on_timeout: OnTimeout::Stop }
}
fn http(method: Method, url: String) -> Dest {
    Dest::Http(HttpDest { method, url, headers: vec![], timeout_ms: 5000 })
}

pub(super) struct Run {
    pub(super) io: ClockIo,
    pub(super) result: Result<(), Halt>,
    pub(super) logs: Vec<String>,
    pub(super) vars: Vec<(String, String)>,
}

/// Roda `blocks` como o script de cima, com `integrations` ativas, e depois o "ao terminar".
pub(super) fn run(integrations: Vec<Integration>, blocks: Vec<Block>, others: Vec<Script>) -> Run {
    let mut io = ClockIo::new();
    let stop = AtomicBool::new(false);
    let dbg = Debug::default();
    let mut logs = vec![];
    let result = {
        let mut log = |m: String| logs.push(m);
        let mut it = Interp::new(&mut io, &stop, &others, 1.0, &mut log, &dbg);
        it.start_integrations(&integrations);
        let r = it.run_blocks(&blocks);
        it.finish_integrations();
        r
    };
    let vars = dbg.vars.lock().unwrap().clone();
    Run { io, result, logs, vars }
}

pub(super) fn var<'a>(r: &'a Run, name: &str) -> &'a str {
    r.vars.iter().find(|v| v.0 == name).map_or("<ausente>", |v| v.1.as_str())
}

/// Critério do ticket: o fluxo define `cep`, espera `!endereco_pendente`, lê o logradouro com
/// `json()` e um Se ramifica nesse valor.
#[test]
fn e2e_viacep_flow() {
    let srv = Server::start();
    let viacep = Integration {
        name: "ViaCEP".into(),
        when: When::OnSet(vec!["cep".into()]),
        dest: http(Method::Get, srv.url("/ws/{cep}/json/")),
        payload: Payload::Json(vec!["cep".into()]),
        result: Some("endereco".into()),
    };
    let blocks = vec![
        set("cep", "\"01001000\""),
        // Ainda dentro do disparo: o servidor demora 300 ms, então a resposta não chegou.
        Block::If { cond: expr("endereco_pendente"), then: vec![ty("pendente")], else_: vec![ty("já chegou?")] },
        wait_until("!endereco_pendente", 5000),
        set("rua", r#"json(endereco, "/logradouro")"#),
        Block::If {
            cond: expr(r#"endereco_ok && endereco_status == 200 && rua == "Praça da Sé""#),
            then: vec![ty("rua {rua}")],
            else_: vec![ty("outra: {rua}")],
        },
        set("cep", "\"99999999\""),
        wait_until("!endereco_pendente", 5000),
        Block::If { cond: expr(r#"json(endereco, "/erro") == true"#), then: vec![ty("cep inválido")], else_: vec![ty("?")] },
    ];
    let r = run(vec![viacep], blocks, vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    assert_eq!(r.io.texts(), ["pendente", "rua Praça da Sé", "cep inválido"], "{:?}", r.logs);
    let paths: Vec<String> = srv.reqs().into_iter().map(|q| format!("{} {}", q.method, q.path)).collect();
    assert_eq!(paths, ["GET /ws/01001000/json/", "GET /ws/99999999/json/"]);
    // O painel Execuções mostra Debug::vars: as variáveis de resultado estão lá.
    assert_eq!(var(&r, "endereco_ok"), "true");
    assert_eq!(var(&r, "endereco_status"), "200");
    assert_eq!(var(&r, "endereco_pendente"), "false");
    assert_eq!(var(&r, "endereco"), r#""{"erro":true}""#);
}

#[test]
fn e2e_slow_server_does_not_block_flow() {
    let srv = Server::start();
    let lento = Integration { name: "lento".into(), when: When::OnSet(vec!["x".into()]), dest: http(Method::Get, srv.url("/lento")), payload: Payload::Json(vec![]), result: Some("r".into()) };
    let blocks = vec![set("x", "1"), ty("logo depois"), Block::Wait { ms: "100".into(), jitter_ms: 0 }, ty("depois da espera"), wait_until("!r_pendente", 6000), ty("{r}")];
    let r = run(vec![lento], blocks, vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    assert_eq!(r.io.texts(), ["logo depois", "depois da espera", "ok lento"]);
    let t = |k: usize| r.io.typed[k].0;
    assert!(t(0) < 300, "o bloco seguinte esperou {} ms", t(0));
    assert!(t(1) < 600, "a espera de 100 ms levou até {} ms", t(1));
    assert!(t(2) >= 1900, "a resposta lenta chegou cedo demais ({} ms)", t(2));
}

#[test]
fn e2e_every_and_on_end() {
    let srv = Server::start();
    let batida = Integration {
        name: "batida".into(),
        when: When::Every { s: 0.25 },
        dest: http(Method::Post, srv.url("/batida")),
        payload: Payload::Csv(vec!["n".into()]),
        result: None,
    };
    let fim = Integration { name: "fim".into(), when: When::OnEnd, dest: http(Method::Post, srv.url("/fim")), payload: Payload::Text("terminou com n={n}".into()), result: Some("fim".into()) };
    let blocks = vec![set("n", "0"), Block::Repeat { times: "4".into(), body: vec![Block::Wait { ms: "300".into(), jitter_ms: 0 }, set("n", "n + 1")] }];
    let r = run(vec![batida, fim], blocks, vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    let reqs = srv.reqs();
    let batidas = reqs.iter().filter(|q| q.path == "/batida").count();
    assert!((3..=6).contains(&batidas), "{batidas} batidas em ~1,2 s: {reqs:?}");
    let fins: Vec<&Req> = reqs.iter().filter(|q| q.path == "/fim").collect();
    assert_eq!(fins.len(), 1, "{reqs:?}");
    assert_eq!(fins[0].body, "terminou com n=4");
    // "Ao terminar" é esperado: a resposta já está nas variáveis quando a execução acaba.
    assert_eq!(var(&r, "fim"), "\"terminou com n=4\"");
    assert_eq!(var(&r, "fim_ok"), "true");
}

#[test]
fn e2e_payload_formats_on_the_wire() {
    let srv = Server::start();
    let mk = |name: &str, method, payload| Integration { name: name.into(), when: When::OnSet(vec!["go".into()]), dest: http(method, srv.url(&format!("/{name}"))), payload, result: Some(name.into()) };
    let list = vec![
        mk("j", Method::Post, Payload::Json(vec!["n".into(), "s".into(), "f".into(), "b".into()])),
        mk("c", Method::Put, Payload::Csv(vec!["n".into(), "s".into()])),
        mk("t", Method::Patch, Payload::Text("pedido {n}".into())),
    ];
    let blocks = vec![set("n", "5"), set("s", "\"a,b\""), set("f", "2.5"), set("b", "true"), set("go", "1"), wait_until("!j_pendente && !c_pendente && !t_pendente", 5000)];
    let r = run(list, blocks, vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    let mut reqs = srv.reqs();
    reqs.sort_by(|a, b| a.path.cmp(&b.path));
    let got: Vec<(String, String, String, String)> = reqs.into_iter().map(|q| (q.method, q.path, q.content_type, q.body)).collect();
    let s = |v: &str| v.to_string();
    assert_eq!(
        got,
        [
            (s("PUT"), s("/c"), s("text/csv; charset=utf-8"), s(r#"5,"a,b""#)),
            (s("POST"), s("/j"), s("application/json; charset=utf-8"), s(r#"{"n":5,"s":"a,b","f":2.5,"b":true}"#)),
            (s("PATCH"), s("/t"), s("text/plain; charset=utf-8"), s("pedido 5")),
        ]
    );
    // O servidor ecoa: o corpo da resposta vira a variável de resultado, e números continuam números.
    assert_eq!(var(&r, "j"), r#""{"n":5,"s":"a,b","f":2.5,"b":true}""#);
}

#[test]
fn e2e_errors_go_to_log() {
    let srv = Server::start();
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let list = vec![
        Integration { name: "fora".into(), when: When::OnSet(vec!["go".into()]), dest: http(Method::Get, format!("http://127.0.0.1:{closed}/x")), payload: Payload::Json(vec![]), result: Some("a".into()) },
        Integration { name: "sumiu".into(), when: When::OnSet(vec!["go".into()]), dest: http(Method::Get, srv.url("/404")), payload: Payload::Json(vec![]), result: Some("b".into()) },
    ];
    let r = run(list, vec![set("go", "1"), wait_until("!a_pendente && !b_pendente", 8000)], vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    assert_eq!((var(&r, "a_ok"), var(&r, "a_status")), ("false", "0"));
    assert_eq!((var(&r, "b_ok"), var(&r, "b_status"), var(&r, "b")), ("false", "404", "\"nao achei\""));
    assert!(r.logs.iter().any(|l| l.starts_with("integração 'fora':")), "{:?}", r.logs);
    assert!(r.logs.iter().any(|l| l.starts_with("integração 'sumiu': falhou (status 404)")), "{:?}", r.logs);
}

/// Respostas não disparam integrações; scripts chamados com Chamar não ativam as próprias.
#[test]
fn e2e_no_loops_and_called_scripts_inactive() {
    let srv = Server::start();
    let a = Integration { name: "a".into(), when: When::OnSet(vec!["go".into()]), dest: http(Method::Get, srv.url("/a")), payload: Payload::Json(vec![]), result: Some("ra".into()) };
    let b = Integration { name: "b".into(), when: When::OnSet(vec!["ra".into()]), dest: http(Method::Get, srv.url("/b")), payload: Payload::Json(vec![]), result: None };
    let sub = Script {
        name: "sub".into(),
        blocks: vec![set("y", "1")],
        integrations: vec![Integration { name: "do sub".into(), when: When::OnSet(vec!["y".into()]), dest: http(Method::Get, srv.url("/sub")), payload: Payload::Json(vec![]), result: None }],
        ..Default::default()
    };
    let blocks = vec![set("go", "1"), wait_until("!ra_pendente", 5000), Block::Call { script: "sub".into() }, Block::Wait { ms: "300".into(), jitter_ms: 0 }];
    let r = run(vec![a, b], blocks, vec![sub]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    let paths: Vec<String> = srv.reqs().into_iter().map(|q| q.path).collect();
    assert_eq!(paths, ["/a"]);
}

/// Parar: o "ao terminar" não roda e a resposta pendente é descartada.
#[test]
fn e2e_stop_discards() {
    let srv = Server::start();
    let lento = Integration { name: "lento".into(), when: When::OnSet(vec!["x".into()]), dest: http(Method::Get, srv.url("/lento")), payload: Payload::Json(vec![]), result: Some("r".into()) };
    let fim = Integration { name: "fim".into(), when: When::OnEnd, dest: http(Method::Get, srv.url("/fim")), payload: Payload::Json(vec![]), result: None };
    let mut io = ClockIo::new();
    let stop = Arc::new(AtomicBool::new(false));
    let dbg = Debug::default();
    let s2 = stop.clone();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        s2.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let t0 = Instant::now();
    let mut log = |_m: String| {};
    let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut log, &dbg);
    it.start_integrations(&[lento, fim]);
    assert_eq!(it.run_blocks(&[set("x", "1"), wait_until("!r_pendente", 10000), ty("não chega")]), Err(Halt::Stopped));
    it.finish_integrations();
    drop(it);
    stopper.join().unwrap();
    assert!(t0.elapsed() < Duration::from_millis(1500), "Parar demorou {:?}", t0.elapsed());
    std::thread::sleep(Duration::from_millis(100));
    let paths: Vec<String> = srv.reqs().into_iter().map(|q| q.path).collect();
    assert_eq!(paths, ["/lento"]);
    assert!(io.typed.is_empty());
    let vars = dbg.vars.lock().unwrap().clone();
    assert!(vars.contains(&("r_pendente".to_string(), "true".to_string())), "{vars:?}");
}

/// HTTPS de verdade: confere o repositório de certificados e o proxy do Windows.
#[test]
#[ignore = "precisa de Windows real e internet (HTTPS com o repositório de certificados do sistema); rode com cargo test -- --ignored"]
fn https_real_viacep() {
    let viacep = Integration {
        name: "ViaCEP".into(),
        when: When::OnSet(vec!["cep".into()]),
        dest: Dest::Http(HttpDest { url: "https://viacep.com.br/ws/{cep}/json/".into(), ..Default::default() }),
        payload: Payload::Json(vec!["cep".into()]),
        result: Some("endereco".into()),
    };
    let blocks = vec![set("cep", "\"01001000\""), wait_until("!endereco_pendente", 15000), ty(r#"{json(endereco, "/logradouro")}"#)];
    let r = run(vec![viacep], blocks, vec![]);
    assert_eq!(r.result, Ok(()), "{:?}", r.logs);
    assert_eq!(var(&r, "endereco_status"), "200", "{:?}", r.logs);
    assert_eq!(r.io.texts(), ["Praça da Sé"]);
}
