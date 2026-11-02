//! Destino ODBC: bancos já instalados no PC (SQL Server, MySQL, PostgreSQL, Access…) pelo
//! gerenciador ODBC do próprio Windows (odbc32.dll).
//!
//! O SQL, os parâmetros `:variavel` e o resultado seguem o contrato do SQLite (veja `sql`):
//! SELECT põe as linhas em JSON em `r` e o número de linhas em `r_status`; os outros comandos
//! põem as linhas afetadas em `r_status`.
use super::sql::{self, Cell};
use super::{Body, Destination, Job, Outcome};
use evalexpr::HashMapContext;
use odbc_api::parameter::InputParameter;
use odbc_api::{ConnectionOptions, Cursor, DataType, IntoParameter, Nullable, ResultSetMetadata};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OdbcDest {
    /// String de conexão (`Driver={…};Server=…;…`) ou só o nome de um DSN. Vai como está: as
    /// chaves fazem parte da sintaxe do ODBC (`Driver={SQL Server}`), então não há `{expressão}`.
    pub connection: String,
    /// Um comando SQL, com `:variavel` nos valores. Não aceita `{…}`.
    pub sql: String,
    pub timeout_ms: u64,
}

impl Default for OdbcDest {
    fn default() -> Self {
        OdbcDest { connection: "DSN=MeuBanco".into(), sql: "SELECT 1 AS um".into(), timeout_ms: 15000 }
    }
}

/// Só o nome de um DSN (sem `=`) vira `DSN=nome`; o resto vai como está.
pub fn connection_string(c: &str) -> String {
    let c = c.trim();
    if c.contains('=') { c.to_string() } else { format!("DSN={c}") }
}

impl Destination for OdbcDest {
    fn kind(&self) -> &'static str {
        "Odbc"
    }

    fn validate(&self, path: &str, errs: &mut Vec<String>) {
        if self.connection.trim().is_empty() {
            errs.push(format!("{path}.connection: vazia (use um DSN ou uma string de conexão)"));
        }
        sql::validate_sql(path, &self.sql, errs);
        if self.timeout_ms == 0 {
            errs.push(format!("{path}.timeout_ms: deve ser > 0"));
        }
    }

    fn prepare(&self, _body: Body, vars: &HashMapContext) -> Result<Job, String> {
        let cs = connection_string(&self.connection);
        let (text, params) = sql::bind(&self.sql, vars)?;
        let timeout = self.timeout_ms;
        Ok(Box::new(move || match exec(&cs, &text, &params, timeout) {
            Ok((n, body)) => Outcome { ok: true, status: n, body, error: None },
            Err(e) => Outcome::failed(format!("ODBC: {e}")),
        }))
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

fn param(c: &Cell) -> Box<dyn InputParameter> {
    match c {
        Cell::Null => Box::new(Option::<String>::None.into_parameter()),
        Cell::Int(i) => Box::new(*i),
        Cell::Float(f) => Box::new(*f),
        Cell::Text(s) => Box::new(s.clone().into_parameter()),
        Cell::Bytes(b) => Box::new(b.clone().into_parameter()),
    }
}

/// Mensagem de erro sem quebras de linha (o log é de uma linha por entrada).
fn msg(e: odbc_api::Error) -> String {
    e.to_string().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lê uma célula conforme o tipo da coluna: inteiros e reais viram números, DECIMAL/NUMERIC
/// viram número quando cabem, binários viram bytes e o resto vira texto (UTF-16).
fn cell(row: &mut odbc_api::CursorRow<'_>, col: u16, t: DataType) -> Result<Cell, odbc_api::Error> {
    Ok(match t {
        DataType::Integer | DataType::SmallInt | DataType::BigInt | DataType::TinyInt | DataType::Bit => {
            let mut v = Nullable::<i64>::null();
            row.get_data(col, &mut v)?;
            v.into_opt().map_or(Cell::Null, Cell::Int)
        }
        DataType::Real | DataType::Double | DataType::Float { .. } => {
            let mut v = Nullable::<f64>::null();
            row.get_data(col, &mut v)?;
            v.into_opt().map_or(Cell::Null, Cell::Float)
        }
        DataType::Binary { .. } | DataType::Varbinary { .. } | DataType::LongVarbinary { .. } => {
            let mut b = vec![];
            if row.get_binary(col, &mut b)? { Cell::Bytes(b) } else { Cell::Null }
        }
        t => {
            let mut w = vec![];
            if !row.get_wide_text(col, &mut w)? {
                return Ok(Cell::Null);
            }
            let s = String::from_utf16_lossy(&w);
            match t {
                DataType::Numeric { scale: 0, .. } | DataType::Decimal { scale: 0, .. } => s.trim().parse().map_or(Cell::Text(s), Cell::Int),
                DataType::Numeric { .. } | DataType::Decimal { .. } => s.trim().parse().map_or(Cell::Text(s), Cell::Float),
                _ => Cell::Text(s),
            }
        }
    })
}

/// Conecta, roda o comando e desconecta (na thread da integração). Devolve (linhas devolvidas
/// ou afetadas, corpo).
pub fn exec(cs: &str, sql_text: &str, params: &[Cell], timeout_ms: u64) -> Result<(i64, String), String> {
    let env = odbc_api::environment().map_err(|e| format!("gerenciador ODBC indisponível: {}", msg(e)))?;
    let secs = timeout_ms.div_ceil(1000).max(1);
    let opts = ConnectionOptions { login_timeout_sec: Some(secs.min(u32::MAX as u64) as u32), ..Default::default() };
    let conn = env.connect_with_connection_string(cs, opts).map_err(|e| format!("não conectou: {}", msg(e)))?;
    let mut pre = conn.preallocate().map_err(msg)?;
    let _ = pre.set_query_timeout_sec(secs as usize); // nem todo driver aceita
    let ps: Vec<Box<dyn InputParameter>> = params.iter().map(param).collect();
    let Some(mut cursor) = pre.execute(sql_text, ps.as_slice()).map_err(msg)? else {
        let n = pre.row_count().map_err(msg)?.unwrap_or(0) as i64;
        return Ok((n, n.to_string()));
    };
    let n = cursor.num_result_cols().map_err(msg)?.max(0) as u16;
    let mut cols = vec![];
    let mut types = vec![];
    for c in 1..=n {
        cols.push(cursor.col_name(c).map_err(msg)?);
        types.push(cursor.col_data_type(c).map_err(msg)?);
    }
    let mut rows = vec![];
    while let Some(mut row) = cursor.next_row().map_err(msg)? {
        let mut r = vec![];
        for c in 1..=n {
            r.push(cell(&mut row, c, types[c as usize - 1]).map_err(msg)?);
        }
        rows.push(r);
    }
    Ok((rows.len() as i64, sql::rows_to_json(&cols, &rows)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::{expr, run as run_flow, set, ty, var, wait_until};
    use crate::integrations::{Dest, Integration, Payload, When};
    use crate::model::Block;

    fn integ(name: &str, on: &str, cs: &str, sql: &str, result: &str) -> Integration {
        Integration {
            name: name.into(),
            when: When::OnSet(vec![on.into()]),
            dest: Dest::Odbc(OdbcDest { connection: cs.into(), sql: sql.into(), timeout_ms: 5000 }),
            payload: Payload::Json(vec![]),
            result: Some(result.into()),
        }
    }

    #[test]
    fn json_contract_and_validation() {
        let d: Dest = serde_json::from_str(r#"{"Odbc": {"connection": "Driver={SQL Server};Server=.;Database=loja;Trusted_Connection=yes", "sql": "SELECT * FROM t WHERE id = :id", "timeout_ms": 3000}}"#).unwrap();
        assert_eq!(
            d,
            Dest::Odbc(OdbcDest { connection: "Driver={SQL Server};Server=.;Database=loja;Trusted_Connection=yes".into(), sql: "SELECT * FROM t WHERE id = :id".into(), timeout_ms: 3000 })
        );
        assert_eq!(serde_json::from_str::<OdbcDest>("{}").unwrap(), OdbcDest::default());
        assert!(serde_json::from_str::<OdbcDest>(r#"{"dsn": "x"}"#).is_err());
        let mut errs = vec![];
        OdbcDest { connection: " ".into(), sql: "".into(), timeout_ms: 0 }.validate("d", &mut errs);
        // As chaves da string de conexão são do ODBC, não expressões.
        OdbcDest { connection: "Driver={SQL Server};Server=.".into(), sql: "SELECT ?".into(), timeout_ms: 1 }.validate("d", &mut errs);
        assert_eq!(errs.len(), 4, "{errs:?}");
        assert!(errs[0].starts_with("d.connection: vazia") && errs[1] == "d.sql: vazio" && errs[2] == "d.timeout_ms: deve ser > 0", "{errs:?}");
        assert!(errs[3].starts_with("d.sql: use :variavel"), "{errs:?}");
    }

    #[test]
    fn dsn_or_connection_string() {
        assert_eq!(connection_string(" MeuBanco "), "DSN=MeuBanco");
        assert_eq!(connection_string("DSN=MeuBanco;UID=ana"), "DSN=MeuBanco;UID=ana");
        assert_eq!(connection_string("Driver={PostgreSQL Unicode};Server=localhost"), "Driver={PostgreSQL Unicode};Server=localhost");
    }

    /// Critério do ticket: uma falha de conexão vai para o log e deixa `r_ok` falso. Roda no
    /// gerenciador ODBC que houver (no Wine, o odbc32 do Wine; no Windows, o do sistema).
    #[test]
    fn invalid_connection_goes_to_log() {
        let list = vec![
            integ("dsn", "x", "DSN_que_nao_existe_ultratask", "SELECT 1", "r1"),
            integ("driver", "x", "Driver={Driver Que Nao Existe};Server=127.0.0.1;Database=x", "SELECT :x AS x", "r2"),
        ];
        let blocks = vec![set("x", "1"), wait_until("!r1_pendente && !r2_pendente", 15000), Block::If { cond: expr("!r1_ok && !r2_ok"), then: vec![ty("falhou")], else_: vec![] }];
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert_eq!(r.io.texts(), ["falhou"]);
        for k in ["r1", "r2"] {
            assert_eq!((var(&r, &format!("{k}_ok")), var(&r, &format!("{k}_status")), var(&r, k)), ("false", "0", "\"\""));
        }
        assert_eq!(r.logs.len(), 2, "{:?}", r.logs);
        for name in ["dsn", "driver"] {
            assert!(r.logs.iter().any(|l| l.starts_with(&format!("integração '{name}': ODBC: ")) && !l.contains('\n')), "{:?}", r.logs);
        }
    }

    /// Ida e volta num banco ODBC de verdade: cria uma tabela, insere valores de variáveis (um
    /// deles com aspas e `;`), lê de volta, extrai campos com `json()` e apaga a tabela.
    /// Ponha a string de conexão (ou o DSN) em `ULTRATASK_ODBC_TESTE`.
    #[test]
    #[ignore = "precisa de um banco e um driver ODBC; defina ULTRATASK_ODBC_TESTE e rode com --include-ignored"]
    fn real_database_round_trip() {
        let cs = std::env::var("ULTRATASK_ODBC_TESTE").expect("defina ULTRATASK_ODBC_TESTE com um DSN ou uma string de conexão");
        let t = format!("ut_teste_{}", std::process::id());
        let list = vec![
            integ("cria", "cria", &cs, &format!("CREATE TABLE {t} (id INTEGER, nome VARCHAR(100), saldo FLOAT)"), "c"),
            integ("insere", "nome", &cs, &format!("INSERT INTO {t} (id, nome, saldo) VALUES (:id, :nome, :saldo)"), "ins"),
            integ("lê", "ler", &cs, &format!("SELECT id, nome, saldo FROM {t} WHERE id >= :ler ORDER BY id"), "r"),
            integ("apaga", "fim", &cs, &format!("DROP TABLE {t}"), "d"),
        ];
        let blocks = vec![
            set("cria", "1"),
            wait_until("!c_pendente", 20000),
            set("id", "1"),
            set("saldo", "10.5"),
            set("nome", r#""Ana""#),
            wait_until("!ins_pendente", 20000),
            set("id", "2"),
            set("saldo", "-3.25"),
            set("nome", r#""O'Brien; DROP TABLE x; --""#),
            wait_until("!ins_pendente", 20000),
            set("ler", "1"),
            wait_until("!r_pendente", 20000),
            Block::If {
                cond: expr(r#"r_ok && r_status == 2 && json(r, "/0/nome") == "Ana" && json(r, "/1/nome") == "O'Brien; DROP TABLE x; --" && json(r, "/1/id") == 2 && json(r, "/0/saldo") == 10.5"#),
                then: vec![ty("ok")],
                else_: vec![ty("errado")],
            },
            set("fim", "1"),
            wait_until("!d_pendente", 20000),
        ];
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert!(r.logs.is_empty(), "{:?}", r.logs);
        assert_eq!(r.io.texts(), ["ok"], "r = {}", var(&r, "r"));
        assert_eq!((var(&r, "ins_ok"), var(&r, "ins_status"), var(&r, "d_ok")), ("true", "1", "true"));
    }
}
