//! Peças comuns dos destinos SQL (SQLite e ODBC).
//!
//! - Parâmetros: o SQL usa `:variavel`. No disparo, cada `:variavel` vira `?` e o valor da
//!   variável é ligado com o tipo preservado. O texto do SQL nunca recebe valores (sem injeção).
//! - Resultado: as linhas de um SELECT viram uma lista JSON de objetos, na ordem das colunas.
use super::to_json;
use evalexpr::{Context, HashMapContext, Value};

/// Um valor de célula ou de parâmetro, no meio do caminho entre o banco e o JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

impl Cell {
    /// Valor de variável ➜ parâmetro. Booleano vira 1/0 (o SQLite e vários bancos não têm
    /// booleano); uma tupla vira texto JSON.
    pub fn from_value(v: &Value) -> Cell {
        match v {
            Value::Int(i) => Cell::Int(*i),
            Value::Float(f) => Cell::Float(*f),
            Value::String(s) => Cell::Text(s.clone()),
            Value::Boolean(b) => Cell::Int(*b as i64),
            Value::Empty => Cell::Null,
            t @ Value::Tuple(_) => Cell::Text(to_json(t).to_string()),
        }
    }

    fn json(&self) -> serde_json::Value {
        use serde_json::Value as J;
        match self {
            Cell::Null => J::Null,
            Cell::Int(i) => J::from(*i),
            Cell::Float(f) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
            Cell::Text(s) => J::String(s.clone()),
            Cell::Bytes(b) => J::String(b.iter().map(|x| format!("{x:02x}")).collect()),
        }
    }
}

/// Linhas ➜ `[{"col": valor, …}, …]`, mantendo a ordem das colunas. Números continuam
/// números; NULL vira `null`; bytes viram texto hexadecimal.
pub fn rows_to_json(cols: &[String], rows: &[Vec<Cell>]) -> String {
    let rows: Vec<String> = rows
        .iter()
        .map(|r| {
            let f: Vec<String> = cols.iter().zip(r).map(|(c, v)| format!("{}:{}", serde_json::Value::String(c.clone()), v.json())).collect();
            format!("{{{}}}", f.join(","))
        })
        .collect();
    format!("[{}]", rows.join(","))
}

/// Um trecho do SQL: texto que passa como está, ou um parâmetro `:nome`.
#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Sql(&'a str),
    Param(&'a str),
    /// Um `?` fora de texto e comentário.
    Question,
}

/// Separa o SQL em texto e parâmetros, pulando textos ('…', "…", `…`, […]) e comentários
/// (`-- …`, `/* … */`). `::` (conversão do PostgreSQL) não é parâmetro.
fn pieces(sql: &str) -> Vec<Piece<'_>> {
    let b: Vec<(usize, char)> = sql.char_indices().collect();
    let at = |k: usize| b.get(k).map(|x| x.1);
    let pos = |k: usize| b.get(k).map_or(sql.len(), |x| x.0);
    let mut out = vec![];
    let (mut start, mut k) = (0, 0);
    while k < b.len() {
        let c = b[k].1;
        let close = match c {
            '\'' => Some('\''),
            '"' => Some('"'),
            '`' => Some('`'),
            '[' => Some(']'),
            _ => None,
        };
        if let Some(close) = close {
            k += 1;
            while k < b.len() && b[k].1 != close {
                k += 1;
            }
            k += 1; // fecha (aspas dobradas '' viram dois textos seguidos: dá no mesmo)
        } else if c == '-' && at(k + 1) == Some('-') {
            while k < b.len() && b[k].1 != '\n' {
                k += 1;
            }
        } else if c == '/' && at(k + 1) == Some('*') {
            k += 2;
            while k < b.len() && !(b[k].1 == '*' && at(k + 1) == Some('/')) {
                k += 1;
            }
            k += 2;
        } else if c == ':' && at(k + 1) == Some(':') {
            k += 2;
        } else if c == ':' && at(k + 1).is_some_and(|n| n.is_alphabetic() || n == '_') && (k == 0 || b[k - 1].1 != ':') {
            out.push(Piece::Sql(&sql[start..b[k].0]));
            let mut e = k + 1;
            while at(e).is_some_and(|n| n.is_alphanumeric() || n == '_') {
                e += 1;
            }
            out.push(Piece::Param(&sql[b[k + 1].0..pos(e)]));
            start = pos(e);
            k = e;
        } else if c == '?' {
            out.push(Piece::Sql(&sql[start..b[k].0]));
            out.push(Piece::Question);
            start = pos(k + 1);
            k += 1;
        } else {
            k += 1;
        }
    }
    out.push(Piece::Sql(&sql[start.min(sql.len())..]));
    out
}

/// Nomes dos parâmetros `:nome`, na ordem em que aparecem (com repetições).
pub fn param_names(sql: &str) -> Vec<&str> {
    pieces(sql).into_iter().filter_map(|p| if let Piece::Param(n) = p { Some(n) } else { None }).collect()
}

/// SQL com `?` no lugar de cada `:nome`, e os valores atuais das variáveis, na mesma ordem.
pub fn bind(sql: &str, vars: &HashMapContext) -> Result<(String, Vec<Cell>), String> {
    let mut text = String::new();
    let mut vals = vec![];
    for p in pieces(sql) {
        match p {
            Piece::Sql(s) => text += s,
            Piece::Question => return Err("use :variavel no SQL em vez de ?".into()),
            Piece::Param(n) => {
                let v = vars.get_value(n).ok_or_else(|| format!("SQL: a variável '{n}' (:{n}) não existe"))?;
                vals.push(Cell::from_value(v));
                text.push('?');
            }
        }
    }
    Ok((text, vals))
}

/// Problemas comuns do campo `sql`.
pub fn validate_sql(path: &str, sql: &str, errs: &mut Vec<String>) {
    if sql.trim().is_empty() {
        errs.push(format!("{path}.sql: vazio"));
    }
    if pieces(sql).contains(&Piece::Question) {
        errs.push(format!("{path}.sql: use :variavel em vez de ? (os valores das variáveis são ligados pelo nome)"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::ctx;

    #[test]
    fn params_skip_strings_comments_and_casts() {
        let sql = "SELECT ':nao', \"a:b\", [x:y], `c:d` FROM t -- :comentario\nWHERE a = :a AND b=:b_2 /* :nada */ AND c = x::int AND d = :preço AND :a > 0";
        assert_eq!(param_names(sql), ["a", "b_2", "preço", "a"]);
        assert_eq!(param_names("INSERT INTO t VALUES (:x,:y)"), ["x", "y"]);
        assert_eq!(param_names("SELECT 'it''s :x' WHERE 1 = :y"), ["y"]);
        assert_eq!(param_names("SELECT 12:30"), Vec::<&str>::new());
    }

    #[test]
    fn bind_keeps_types_and_never_interpolates() {
        let vars = ctx(&[
            ("n", Value::Int(3)),
            ("p", Value::Float(2.5)),
            ("s", Value::String("x'); DROP TABLE t; --".into())),
            ("b", Value::Boolean(true)),
        ]);
        let (q, v) = bind("INSERT INTO t VALUES (:n, :p, :s, :b, ':s') -- :s", &vars).unwrap();
        assert_eq!(q, "INSERT INTO t VALUES (?, ?, ?, ?, ':s') -- :s");
        assert_eq!(v, [Cell::Int(3), Cell::Float(2.5), Cell::Text("x'); DROP TABLE t; --".into()), Cell::Int(1)]);
        assert_eq!(bind("SELECT :falta", &vars).unwrap_err(), "SQL: a variável 'falta' (:falta) não existe");
        assert!(bind("SELECT ?", &vars).is_err());
    }

    #[test]
    fn rows_json_keeps_column_order_and_types() {
        let cols = vec!["z".to_string(), "a".into(), "n".into(), "f".into(), "b".into()];
        let rows = vec![vec![Cell::Text("Zé \"aspas\"".into()), Cell::Null, Cell::Int(7), Cell::Float(1.5), Cell::Bytes(vec![0, 255])]];
        assert_eq!(rows_to_json(&cols, &rows), r#"[{"z":"Zé \"aspas\"","a":null,"n":7,"f":1.5,"b":"00ff"}]"#);
        assert_eq!(rows_to_json(&cols, &[]), "[]");
    }

    #[test]
    fn validate() {
        let mut errs = vec![];
        validate_sql("d", " ", &mut errs);
        validate_sql("d", "SELECT * FROM t WHERE a = ?", &mut errs);
        validate_sql("d", "SELECT '?' FROM t WHERE a = :a", &mut errs);
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs[0] == "d.sql: vazio" && errs[1].starts_with("d.sql: use :variavel"), "{errs:?}");
    }
}
