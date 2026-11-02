//! Destino SQLite: um banco num arquivo .db (criado se faltar), com o SQLite embutido no exe.
//!
//! - O SQL usa `:variavel`; os valores são ligados com o tipo preservado (veja `sql`).
//! - SELECT (ou qualquer comando que devolve colunas): `r` recebe as linhas em JSON e
//!   `r_status` o número de linhas. Outros comandos: `r_status` (e `r`) recebem o número de
//!   linhas afetadas.
//! - Gravações no mesmo arquivo são feitas uma de cada vez dentro do processo e esperam até
//!   `timeout_ms` por outro processo (busy timeout), então não dão "database is locked".
use super::sql::{self, Cell};
use super::{Body, Destination, Job, Outcome, check_interp, eval_text};
use crate::model::interpolate;
use evalexpr::HashMapContext;
use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SqliteDest {
    /// Arquivo .db; aceita `{expressão}`; relativo à pasta de dados.
    pub path: String,
    /// Um comando SQL, com `:variavel` nos valores. Não aceita `{…}`.
    pub sql: String,
    pub timeout_ms: u64,
}

impl Default for SqliteDest {
    fn default() -> Self {
        SqliteDest { path: "dados.db".into(), sql: "SELECT 1 AS um".into(), timeout_ms: 10000 }
    }
}

impl Destination for SqliteDest {
    fn kind(&self) -> &'static str {
        "Sqlite"
    }

    fn validate(&self, path: &str, errs: &mut Vec<String>) {
        if self.path.trim().is_empty() {
            errs.push(format!("{path}.path: vazio"));
        }
        check_interp(errs, &format!("{path}.path"), &self.path);
        sql::validate_sql(path, &self.sql, errs);
        if self.timeout_ms == 0 {
            errs.push(format!("{path}.timeout_ms: deve ser > 0"));
        }
    }

    fn prepare(&self, _body: Body, vars: &HashMapContext) -> Result<Job, String> {
        let p = interpolate(&self.path, |e| eval_text(vars, e))?;
        if p.trim().is_empty() {
            return Err("o caminho do banco ficou vazio".into());
        }
        let path = super::file::resolve(&p);
        let (text, params) = sql::bind(&self.sql, vars)?;
        let timeout = self.timeout_ms;
        Ok(Box::new(move || match exec(&path, &text, &params, timeout) {
            Ok((n, body)) => Outcome { ok: true, status: n, body, error: None },
            Err(e) => Outcome::failed(format!("SQLite \"{}\": {e}", path.display())),
        }))
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

impl ToSql for Cell {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Cell::Null => ValueRef::Null,
            Cell::Int(i) => ValueRef::Integer(*i),
            Cell::Float(f) => ValueRef::Real(*f),
            Cell::Text(s) => ValueRef::Text(s.as_bytes()),
            Cell::Bytes(b) => ValueRef::Blob(b),
        }))
    }
}

fn cell(v: ValueRef<'_>) -> Cell {
    match v {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(i) => Cell::Int(i),
        ValueRef::Real(f) => Cell::Float(f),
        ValueRef::Text(t) => Cell::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Cell::Bytes(b.to_vec()),
    }
}

/// Uma trava por arquivo: dentro do processo, um comando de cada vez no mesmo banco.
fn file_lock(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    // O Windows não diferencia maiúsculas nos caminhos.
    let key = PathBuf::from(path.to_string_lossy().to_lowercase());
    LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()).entry(key).or_default().clone()
}

/// Roda o comando (na thread da integração). Devolve (linhas devolvidas ou afetadas, corpo).
pub fn exec(path: &Path, sql_text: &str, params: &[Cell], timeout_ms: u64) -> Result<(i64, String), String> {
    let lock = file_lock(path);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_millis(timeout_ms)).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(sql_text).map_err(|e| e.to_string())?;
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    if cols.is_empty() {
        let n = stmt.execute(rusqlite::params_from_iter(params)).map_err(|e| e.to_string())?;
        return Ok((n as i64, n.to_string()));
    }
    let mut rows = stmt.query(rusqlite::params_from_iter(params)).map_err(|e| e.to_string())?;
    let mut out = vec![];
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let mut r = vec![];
        for i in 0..cols.len() {
            r.push(cell(row.get_ref(i).map_err(|e| e.to_string())?));
        }
        out.push(r);
    }
    Ok((out.len() as i64, sql::rows_to_json(&cols, &out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::{run as run_flow, set, ty, var, wait_until};
    use crate::integrations::{Dest, Integration, Payload, When};
    use crate::model::Block;

    fn tmp_db(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ultratask-sqlite-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("sub").join("banco.db")
    }

    fn integ(name: &str, on: &str, db: &Path, sql: &str, result: Option<&str>) -> Integration {
        Integration {
            name: name.into(),
            when: When::OnSet(vec![on.into()]),
            dest: Dest::Sqlite(SqliteDest { path: db.display().to_string(), sql: sql.into(), timeout_ms: 5000 }),
            payload: Payload::Json(vec![]),
            result: result.map(Into::into),
        }
    }

    #[test]
    fn json_contract_and_validation() {
        let d: Dest = serde_json::from_str(r#"{"Sqlite": {"path": "dados/{loja}.db", "sql": "SELECT * FROM t WHERE id = :id", "timeout_ms": 3000}}"#).unwrap();
        assert_eq!(d, Dest::Sqlite(SqliteDest { path: "dados/{loja}.db".into(), sql: "SELECT * FROM t WHERE id = :id".into(), timeout_ms: 3000 }));
        assert_eq!(serde_json::from_str::<SqliteDest>("{}").unwrap(), SqliteDest::default());
        assert!(serde_json::from_str::<SqliteDest>(r#"{"path": "a.db", "query": "x"}"#).is_err());
        let mut errs = vec![];
        SqliteDest { path: "{x +}".into(), sql: "SELECT ?".into(), timeout_ms: 0 }.validate("d", &mut errs);
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(errs[0].starts_with("d.path: {x +}") && errs[1].starts_with("d.sql: use :variavel") && errs[2] == "d.timeout_ms: deve ser > 0", "{errs:?}");
    }

    /// Critério do ticket, pelo fluxo: insere valores de variáveis (com os tipos), lê de volta
    /// para uma variável, extrai um campo com `json()`, e um valor com aspas e `;` não quebra
    /// nem injeta SQL.
    #[test]
    fn insert_select_json_and_no_injection() {
        let db = tmp_db("fluxo");
        let list = vec![
            integ("cria", "cria", &db, "CREATE TABLE IF NOT EXISTS clientes (id INTEGER PRIMARY KEY, nome TEXT, saldo REAL, vip INTEGER, obs TEXT)", Some("c")),
            integ("insere", "nome", &db, "INSERT INTO clientes (nome, saldo, vip, obs) VALUES (:nome, :saldo, :vip, ':nome fica como texto')", Some("ins")),
            integ("lê", "ler", &db, "SELECT id, nome, saldo, vip, obs FROM clientes WHERE nome = :nome OR id > :ler ORDER BY id", Some("r")),
            integ("conta", "contar", &db, "SELECT count(*) AS n FROM clientes", Some("q")),
        ];
        let blocks = vec![
            set("cria", "1"),
            wait_until("!c_pendente", 5000),
            set("saldo", "10.5"),
            set("vip", "true"),
            set("nome", r#""Ana""#),
            wait_until("!ins_pendente", 5000),
            set("saldo", "-3.25"),
            set("vip", "false"),
            // Aspas, ponto e vírgula e um DROP: tem que virar só texto.
            set("nome", r#""O'Brien \"Zé\"; DROP TABLE clientes; --""#),
            wait_until("!ins_pendente", 5000),
            set("ler", "0"),
            wait_until("!r_pendente", 5000),
            set("primeiro", r#"json(r, "/0/nome")"#),
            set("saldo2", r#"json(r, "/1/saldo")"#),
            Block::If { cond: crate::integrations::tests::expr(r#"primeiro == "Ana" && json(r, "/0/saldo") == 10.5 && json(r, "/0/vip") == 1"#), then: vec![ty("ok {primeiro}")], else_: vec![ty("errado")] },
            set("contar", "1"),
            wait_until("!q_pendente", 5000),
        ];
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert!(r.logs.is_empty(), "{:?}", r.logs);
        assert_eq!(r.io.texts(), ["ok Ana"]);
        assert_eq!((var(&r, "ins_ok"), var(&r, "ins_status"), var(&r, "ins")), ("true", "1", "\"1\""));
        assert_eq!((var(&r, "r_ok"), var(&r, "r_status")), ("true", "2"));
        assert_eq!(
            var(&r, "r"),
            r#""[{"id":1,"nome":"Ana","saldo":10.5,"vip":1,"obs":":nome fica como texto"},{"id":2,"nome":"O'Brien \"Zé\"; DROP TABLE clientes; --","saldo":-3.25,"vip":0,"obs":":nome fica como texto"}]""#
        );
        assert_eq!(var(&r, "saldo2"), "-3.25"); // número continua número
        // A tabela continua lá, com as duas linhas.
        assert_eq!(var(&r, "q"), r#""[{"n":2}]""#);
        let _ = std::fs::remove_dir_all(db.parent().unwrap().parent().unwrap());
    }

    /// Duas integrações gravando no mesmo arquivo ao mesmo tempo, muitas vezes: nenhuma dá
    /// "database is locked".
    #[test]
    fn concurrent_writers_do_not_lock() {
        let db = tmp_db("concorrencia");
        exec(&db, "CREATE TABLE t (quem TEXT, n INTEGER)", &[], 5000).unwrap();
        let list = vec![
            integ("a", "n", &db, "INSERT INTO t VALUES ('a', :n)", Some("ra")),
            integ("b", "n", &db, "INSERT INTO t VALUES ('b', :n)", Some("rb")),
            integ("c", "n", &db, "INSERT INTO t VALUES ('c', :n)", None),
        ];
        let mut blocks = vec![];
        for k in 0..40 {
            blocks.push(set("n", &k.to_string()));
            blocks.push(wait_until("!ra_pendente && !rb_pendente", 5000));
        }
        blocks.push(Block::Wait { ms: "300".into(), jitter_ms: 0 });
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert!(r.logs.is_empty(), "{:?}", r.logs);
        // Além disso, direto por threads, sem o fluxo no meio: 4 threads x 50 gravações.
        let hs: Vec<_> = (0..4)
            .map(|t| {
                let db = db.clone();
                std::thread::spawn(move || (0..50).map(|k| exec(&db, "INSERT INTO t VALUES (?, ?)", &[Cell::Text(format!("t{t}")), Cell::Int(k)], 5000)).filter(|x| x.is_err()).count())
            })
            .collect();
        let falhas: usize = hs.into_iter().map(|h| h.join().unwrap()).sum();
        assert_eq!(falhas, 0);
        let (n, body) = exec(&db, "SELECT count(*) AS n FROM t", &[], 5000).unwrap();
        assert_eq!((n, body.as_str()), (1, r#"[{"n":320}]"#));
        let _ = std::fs::remove_dir_all(db.parent().unwrap().parent().unwrap());
    }

    /// Mesmo com outro processo (outra conexão, fora da trava interna) segurando o banco por um
    /// tempo, a gravação espera (busy timeout) em vez de falhar.
    #[test]
    fn waits_for_other_connection() {
        let db = tmp_db("outra");
        exec(&db, "CREATE TABLE t (n INTEGER)", &[], 5000).unwrap();
        let other = Connection::open(&db).unwrap();
        other.execute_batch("BEGIN EXCLUSIVE; INSERT INTO t VALUES (0);").unwrap();
        let db2 = db.clone();
        let h = std::thread::spawn(move || exec(&db2, "INSERT INTO t VALUES (1)", &[], 5000));
        std::thread::sleep(Duration::from_millis(400));
        other.execute_batch("COMMIT").unwrap();
        assert_eq!(h.join().unwrap(), Ok((1, "1".into())));
        assert!(exec(&db, "INSERT INTO t VALUES (2)", &[], 1).is_ok());
        let _ = std::fs::remove_dir_all(db.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn errors_go_to_log() {
        let db = tmp_db("erros");
        let list = vec![
            integ("sem tabela", "x", &db, "SELECT * FROM nao_existe", Some("r1")),
            integ("sem variável", "x", &db, "SELECT :nao_definida", Some("r2")),
        ];
        let r = run_flow(list, vec![set("x", "1"), wait_until("!r1_pendente && !r2_pendente", 5000)], vec![]);
        assert_eq!((var(&r, "r1_ok"), var(&r, "r1_status"), var(&r, "r2_ok")), ("false", "0", "false"));
        assert!(r.logs.iter().any(|l| l.starts_with("integração 'sem tabela': SQLite") && l.contains("no such table: nao_existe")), "{:?}", r.logs);
        assert!(r.logs.iter().any(|l| l == "integração 'sem variável': SQL: a variável 'nao_definida' (:nao_definida) não existe"), "{:?}", r.logs);
        let _ = std::fs::remove_dir_all(db.parent().unwrap().parent().unwrap());
    }
}
