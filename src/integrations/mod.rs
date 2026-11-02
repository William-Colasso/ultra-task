//! Integrações: o script conversa com o mundo em paralelo ao fluxo de blocos.
//!
//! Cada integração tem um gatilho (`When`), um destino (`Dest`), um formato de envio
//! (`Payload`) e uma variável de resultado opcional. O fluxo nunca espera por elas: o
//! envio roda numa thread e a resposta volta pelas variáveis `r`, `r_ok`, `r_status` e
//! `r_pendente` (veja `engine`).
//!
//! **Acrescentar um destino** (tickets 10 a 13): uma variante nova em `Dest`, um módulo
//! que implementa `Destination` para ela, um braço em `Dest::get` e um em `editor::dest_ui`.
//! O resto (gatilhos, formatos, variáveis de resultado, validação comum) não muda.

pub mod editor;
pub mod engine;
pub mod file;
pub mod http;

use crate::model::{expr_error, interpolate, is_ident};
use evalexpr::{Context, HashMapContext, Value};
use serde::{Deserialize, Serialize};

pub use file::{FileDest, FileMode};
pub use http::{HttpDest, Method};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Integration {
    pub name: String,
    pub when: When,
    pub dest: Dest,
    pub payload: Payload,
    /// Variável que recebe a resposta; também cria `<r>_ok`, `<r>_status` e `<r>_pendente`.
    pub result: Option<String>,
}

impl Default for Integration {
    fn default() -> Self {
        Integration { name: "Nova integração".into(), when: When::OnSet(vec![]), dest: Dest::Http(HttpDest::default()), payload: Payload::Json(vec![]), result: None }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum When {
    /// Quando um bloco define uma destas variáveis (respostas de integrações não contam).
    OnSet(Vec<String>),
    /// A cada `s` segundos, enquanto o script roda (pula se o envio anterior ainda está pendente).
    Every { s: f64 },
    /// Uma vez, depois do último bloco (não roda se o usuário parou o script).
    OnEnd,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Dest {
    Http(HttpDest),
    File(FileDest),
}

impl Dest {
    pub fn get(&self) -> &dyn Destination {
        match self {
            Dest::Http(h) => h,
            Dest::File(f) => f,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Payload {
    /// Objeto JSON `{"var": valor, …}`; números continuam números.
    Json(Vec<String>),
    /// Uma linha CSV com os valores, na ordem dada (sem quebra de linha no fim).
    Csv(Vec<String>),
    /// Texto livre com `{expressão}`.
    Text(String),
}

/// O que vai ser enviado, já com os valores do instante do disparo.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub content_type: &'static str,
    pub text: String,
    /// Linha de cabeçalho do CSV (os nomes das variáveis); `None` nos outros formatos.
    pub header: Option<String>,
}

/// Como terminou uma entrega. Vira `r`, `r_ok` e `r_status`.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub ok: bool,
    /// Status HTTP, ou 0 quando não houve resposta.
    pub status: i64,
    pub body: String,
    /// Falha de rede/E-S; vai para o log.
    pub error: Option<String>,
}

impl Outcome {
    pub fn failed(e: impl Into<String>) -> Self {
        Outcome { ok: false, status: 0, body: String::new(), error: Some(e.into()) }
    }
}

/// Trabalho feito numa thread separada; nunca toca nas variáveis do fluxo.
pub type Job = Box<dyn FnOnce() -> Outcome + Send>;

/// Interface de todo destino.
pub trait Destination {
    /// Nome da variante no JSON (`"Http"`), para os caminhos de erro.
    fn kind(&self) -> &'static str;
    /// Problemas de configuração, cada um prefixado por `path` (ex.: `integrations[0].dest.Http`).
    fn validate(&self, path: &str, errs: &mut Vec<String>);
    /// Roda na thread do fluxo, no instante do disparo: resolve os `{…}` com `eval` e devolve o
    /// trabalho que vai rodar em segundo plano.
    fn prepare(&self, body: Body, eval: &mut dyn FnMut(&str) -> Result<String, String>) -> Result<Job, String>;
    /// Limite de uma entrega; também limita a espera do gatilho "ao terminar".
    fn timeout_ms(&self) -> u64;
}

/// Nomes das variáveis criadas para o resultado `r`.
pub fn result_vars(r: &str) -> [String; 4] {
    [r.to_string(), format!("{r}_ok"), format!("{r}_status"), format!("{r}_pendente")]
}

// ---------- formatos de envio ----------

fn var<'c>(vars: &'c HashMapContext, name: &str) -> Result<&'c Value, String> {
    vars.get_value(name).ok_or_else(|| format!("variável '{name}' não existe"))
}

fn to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::String(s) => J::String(s.clone()),
        Value::Int(i) => J::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
        Value::Boolean(b) => J::Bool(*b),
        Value::Tuple(t) => J::Array(t.iter().map(to_json).collect()),
        Value::Empty => J::Null,
    }
}

fn csv_field(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Empty => String::new(),
        v => v.to_string(),
    };
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
}

/// Texto de uma expressão como o `Type`/`Log` mostram: strings sem aspas.
pub fn eval_text(vars: &HashMapContext, e: &str) -> Result<String, String> {
    match evalexpr::eval_with_context(e.trim(), vars) {
        Ok(Value::String(s)) => Ok(s),
        Ok(v) => Ok(v.to_string()),
        Err(err) => Err(format!("expressão '{e}': {err}")),
    }
}

impl Payload {
    pub fn render(&self, vars: &HashMapContext) -> Result<Body, String> {
        Ok(match self {
            Payload::Json(names) => {
                // Montado à mão para manter a ordem escolhida (o Map do serde_json ordena as chaves).
                let mut fields = vec![];
                for n in names {
                    fields.push(format!("{}:{}", serde_json::Value::String(n.clone()), to_json(var(vars, n)?)));
                }
                Body { content_type: "application/json; charset=utf-8", text: format!("{{{}}}", fields.join(",")), header: None }
            }
            Payload::Csv(names) => {
                let fields: Result<Vec<String>, String> = names.iter().map(|n| var(vars, n).map(csv_field)).collect();
                let header = names.iter().map(|n| csv_field(&Value::String(n.clone()))).collect::<Vec<_>>().join(",");
                Body { content_type: "text/csv; charset=utf-8", text: fields?.join(","), header: Some(header) }
            }
            Payload::Text(t) => Body { content_type: "text/plain; charset=utf-8", text: interpolate(t, |e| eval_text(vars, e))?, header: None },
        })
    }
}

// ---------- json(texto, "/caminho") ----------

/// Lê um campo de um texto JSON por JSON Pointer (RFC 6901). Strings, números e booleanos voltam
/// como valores; objetos e listas voltam como texto JSON (dá para encadear `json(json(r, "/a"), "/b")`);
/// campo ausente, `null` ou texto vazio (resposta que não chegou) voltam como `""`.
pub fn json_get(text: &str, pointer: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::String(String::new()));
    }
    let doc: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("json(): o texto não é JSON válido ({e})"))?;
    let p = if pointer.is_empty() || pointer.starts_with('/') { pointer.to_string() } else { format!("/{pointer}") };
    Ok(match doc.pointer(&p) {
        None | Some(serde_json::Value::Null) => Value::String(String::new()),
        Some(serde_json::Value::String(s)) => Value::String(s.clone()),
        Some(serde_json::Value::Bool(b)) => Value::Boolean(*b),
        Some(serde_json::Value::Number(n)) => n.as_i64().map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::Int),
        Some(v) => Value::String(v.to_string()),
    })
}

// ---------- validação ----------

/// Erro de `{…}` num texto interpolado, se houver.
pub fn interp_error(text: &str) -> Option<String> {
    interpolate(text, |e| expr_error(e).map_or(Ok(String::new()), |m| Err(format!("{{{e}}}: {m}")))).err()
}

pub fn check_interp(errs: &mut Vec<String>, path: &str, text: &str) {
    if let Some(e) = interp_error(text) {
        errs.push(format!("{path}: {e}"));
    }
}

fn check_names(errs: &mut Vec<String>, path: &str, names: &[String]) {
    for (k, n) in names.iter().enumerate() {
        if !is_ident(n) {
            errs.push(format!("{path}[{k}]: \"{n}\" não é um nome de variável válido"));
        }
    }
}

/// Problemas das integrações, cada um com o caminho JSON (`integrations[i].…`).
pub fn validate(list: &[Integration], errs: &mut Vec<String>) {
    for (i, it) in list.iter().enumerate() {
        let p = |f: &str| format!("integrations[{i}].{f}");
        if it.name.trim().is_empty() {
            errs.push(format!("{}: vazio", p("name")));
        }
        if let Some(r) = &it.result
            && !is_ident(r)
        {
            errs.push(format!("{}: \"{r}\" não é um nome de variável válido", p("result")));
        }
        match &it.when {
            When::OnSet(names) => {
                if names.is_empty() {
                    errs.push(format!("{}: escolha ao menos uma variável", p("when.OnSet")));
                }
                check_names(errs, &p("when.OnSet"), names);
                if let Some(r) = &it.result {
                    let own = result_vars(r);
                    for n in names.iter().filter(|n| own.contains(n)) {
                        errs.push(format!("{}: \"{n}\" é a própria variável de resultado; a integração dispararia a si mesma", p("when.OnSet")));
                    }
                }
            }
            When::Every { s } if !(*s > 0.0) => errs.push(format!("{}: deve ser > 0 (é {s})", p("when.Every.s"))),
            _ => {}
        }
        match &it.payload {
            Payload::Json(names) => check_names(errs, &p("payload.Json"), names),
            Payload::Csv(names) => {
                if names.is_empty() {
                    errs.push(format!("{}: escolha ao menos uma variável", p("payload.Csv")));
                }
                check_names(errs, &p("payload.Csv"), names)
            }
            Payload::Text(t) => check_interp(errs, &p("payload.Text"), t),
        }
        let d = it.dest.get();
        d.validate(&p(&format!("dest.{}", d.kind())), errs);
    }
}

#[cfg(test)]
mod tests;
