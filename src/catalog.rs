//! Catálogo das funções e operadores de expressão, e a lógica pura da cola
//! (palavra sob o cursor, `{…}` nos textos, filtro de sugestões, variáveis do script).
//! A UI (`cola.rs`, `manual.rs`) só chama o que está aqui.

use crate::model::{self, Block, Script};
use crate::uia::UiaAction;

/// Uma função que pode ser usada nas expressões.
pub struct FnDoc {
    pub name: &'static str,
    pub sig: &'static str,
    pub desc: &'static str,
    /// Expressão completa e autossuficiente; o teste roda cada uma num contexto novo.
    pub example: &'static str,
    /// `true` = registrada pelo UltraTask (`runner::custom_functions`); `false` = nativa do evalexpr.
    pub ours: bool,
}

const fn f(name: &'static str, sig: &'static str, desc: &'static str, example: &'static str, ours: bool) -> FnDoc {
    FnDoc { name, sig, desc, example, ours }
}

pub const FUNCTIONS: &[FnDoc] = &[
    f("rand", "rand(a, b)", "inteiro sorteado entre a e b, inclusive", "rand(1, 6)", true),
    f("contem", "contem(texto, trecho)", "verdadeiro se o texto contém o trecho (sem diferenciar maiúsculas)", "contem(\"Olá Mundo\", \"mundo\")", true),
    f("json", "json(texto, \"/caminho\")", "campo de um JSON por JSON Pointer (ex.: resposta de integração)", "json(\"{\\\"a\\\": [1, 2]}\", \"/a/1\")", true),
    f("min", "min(a, b, …)", "o menor dos valores", "min(3, 1, 2)", false),
    f("max", "max(a, b, …)", "o maior dos valores", "max(3, 1, 2)", false),
    f("floor", "floor(x)", "arredonda para baixo", "floor(2.7)", false),
    f("round", "round(x)", "arredonda para o mais próximo", "round(2.5)", false),
    f("ceil", "ceil(x)", "arredonda para cima", "ceil(2.1)", false),
    f("len", "len(texto)", "tamanho do texto em bytes (letras acentuadas contam 2)", "len(\"abc\")", false),
    f("if", "if(cond, a, b)", "a se cond for verdadeira, senão b", "if(2 > 1, \"sim\", \"não\")", false),
    f("str::to_lowercase", "str::to_lowercase(texto)", "texto em minúsculas", "str::to_lowercase(\"ABC\")", false),
    f("str::to_uppercase", "str::to_uppercase(texto)", "texto em maiúsculas", "str::to_uppercase(\"abc\")", false),
    f("str::trim", "str::trim(texto)", "tira os espaços das pontas", "str::trim(\"  oi  \")", false),
    f("str::from", "str::from(valor)", "converte um valor em texto", "str::from(42) + \"!\"", false),
    f("str::substring", "str::substring(texto, início, fim)", "trecho do texto entre as posições (em bytes, a partir de 0; fim é opcional)", "str::substring(\"abcdef\", 1, 3)", false),
    f("math::abs", "math::abs(x)", "valor absoluto", "math::abs(-5)", false),
    f("math::sqrt", "math::sqrt(x)", "raiz quadrada", "math::sqrt(16)", false),
];

/// Um operador (ou literal) das expressões.
pub struct OpDoc {
    pub op: &'static str,
    pub desc: &'static str,
    pub example: &'static str,
}

const fn o(op: &'static str, desc: &'static str, example: &'static str) -> OpDoc {
    OpDoc { op, desc, example }
}

pub const OPERATORS: &[OpDoc] = &[
    o("3  2.5", "números inteiros e decimais", "2.5 * 2"),
    o("\"texto\"", "texto entre aspas (no JSON: \\\"texto\\\")", "\"oi\""),
    o("true  false", "verdadeiro e falso", "true"),
    o("+ - * /", "soma, subtração, multiplicação, divisão (a de inteiros trunca)", "7 / 2"),
    o("%", "resto da divisão", "7 % 3"),
    o("^", "potência", "2 ^ 10"),
    o("+ (textos)", "junta dois textos", "\"a\" + \"b\""),
    o("== !=", "igual, diferente", "1 + 1 == 2"),
    o("< > <= >=", "comparações", "3 >= 2"),
    o("&& || !", "e, ou, não", "true && !false"),
    o("( )", "agrupa", "(1 + 2) * 3"),
];

/// Sintaxe dos campos de texto (Digitar, Mostrar no log).
pub const TEXT_SYNTAX: &[(&str, &str)] = &[
    ("{expressão}", "é trocado pelo valor da expressão; ex.: \"pedido {n + 1}\""),
    ("{{  }}", "chaves literais"),
];

// ---------- cola ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldKind {
    /// O campo inteiro é uma expressão (x/y, tempos, Definir variável, condição).
    Expr,
    /// Texto com `{expressão}` (Digitar, Mostrar no log).
    Text,
}

/// O que a cola precisa saber sobre o campo em foco. Índices em caracteres (como o cursor do egui).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Query {
    /// A expressão sendo editada (o campo inteiro, ou o miolo das chaves).
    pub expr: String,
    /// Onde começa a palavra que o usuário está digitando.
    pub word_start: usize,
    pub cursor: usize,
    /// Palavra antes do cursor (letras, dígitos, `_` e `::`).
    pub prefix: String,
    /// O cursor está dentro de um texto entre aspas: não sugere nada.
    pub in_string: bool,
}

impl Query {
    pub fn error(&self) -> Option<String> {
        model::expr_error(&self.expr)
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == ':'
}

/// Intervalo `[início, fim)` do miolo do `{…}` que contém o cursor, em caracteres.
/// `{{`/`}}` fora de expressão são chaves literais. Uma chave aberta e ainda não fechada vale até o fim.
pub fn brace_span(text: &str, cursor: usize) -> Option<(usize, usize)> {
    let c: Vec<char> = text.chars().collect();
    let (mut i, mut open) = (0, None);
    while i < c.len() {
        match (open, c[i]) {
            (None, '{') if c.get(i + 1) == Some(&'{') => i += 1,
            (None, '}') if c.get(i + 1) == Some(&'}') => i += 1,
            (None, '{') => open = Some(i + 1),
            (Some(s), '}') => {
                if (s..=i).contains(&cursor) {
                    return Some((s, i));
                }
                open = None;
            }
            _ => {}
        }
        i += 1;
    }
    open.filter(|&s| cursor >= s).map(|s| (s, c.len()))
}

/// Monta a consulta da cola, ou `None` se ela não deve aparecer (texto fora das chaves).
pub fn query(text: &str, cursor: usize, kind: FieldKind) -> Option<Query> {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let (s, e) = match kind {
        FieldKind::Expr => (0, chars.len()),
        FieldKind::Text => brace_span(text, cursor)?,
    };
    let before = &chars[s..cursor];
    let mut quotes = 0;
    for (i, &ch) in before.iter().enumerate() {
        if ch == '"' && (i == 0 || before[i - 1] != '\\') {
            quotes += 1;
        }
    }
    let n = before.iter().rev().take_while(|&&c| is_word(c)).count();
    Some(Query {
        expr: chars[s..e].iter().collect(),
        word_start: cursor - n,
        cursor,
        prefix: chars[cursor - n..cursor].iter().collect(),
        in_string: quotes % 2 == 1,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SugKind {
    Var,
    Fn,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Suggestion {
    pub kind: SugKind,
    /// O que a lista mostra (nome da variável ou assinatura da função).
    pub label: String,
    /// O que entra no lugar da palavra: `contador` ou `rand(`.
    pub insert: String,
    pub desc: String,
}

/// Variáveis e funções que casam com a palavra sendo digitada (sem diferenciar maiúsculas;
/// `trim` também acha `str::trim`). Palavra vazia lista tudo; número ou texto entre aspas, nada.
pub fn suggestions(q: &Query, vars: &[String]) -> Vec<Suggestion> {
    if q.in_string || q.prefix.starts_with(|c: char| c.is_ascii_digit()) {
        return vec![];
    }
    let p = q.prefix.to_lowercase();
    let hit = |name: &str| {
        let n = name.to_lowercase();
        n.starts_with(&p) || n.rsplit("::").next().is_some_and(|last| last.starts_with(&p))
    };
    let vars = vars.iter().filter(|v| hit(v) && **v != q.prefix).map(|v| Suggestion { kind: SugKind::Var, label: v.clone(), insert: v.clone(), desc: "variável do script".into() });
    let fns = FUNCTIONS.iter().filter(|d| hit(d.name)).map(|d| Suggestion { kind: SugKind::Fn, label: d.sig.into(), insert: format!("{}(", d.name), desc: d.desc.into() });
    vars.chain(fns).collect()
}

/// Troca a palavra `[word_start, cursor)` por `insert`; devolve o texto novo e o cursor depois do inserido.
pub fn apply(text: &str, word_start: usize, cursor: usize, insert: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let word_start = word_start.min(cursor);
    let mut out: String = chars[..word_start].iter().collect();
    out += insert;
    out.extend(&chars[cursor..]);
    (out, word_start + insert.chars().count())
}

/// Variáveis que um bloco define (sem descer nos filhos).
fn block_vars(b: &Block) -> Vec<&str> {
    match b {
        Block::SetVar { name, .. } | Block::OcrToVar { name, .. } => vec![name.as_str()],
        Block::Uia { action: UiaAction::ReadValue(name) | UiaAction::ReadName(name), .. } => vec![name.as_str()],
        // Ticket 09 (integrações): as variáveis de resultado dos blocos de integração entram aqui.
        _ => vec![],
    }
}

fn children(b: &Block) -> Vec<&[Block]> {
    match b {
        Block::If { then, else_, .. } => vec![then, else_],
        Block::Repeat { body, .. } | Block::While { body, .. } => vec![body],
        _ => vec![],
    }
}

/// Variáveis definidas em qualquer nível do script, e nos scripts que ele chama (as variáveis são
/// globais na execução, inclusive dentro de `Call`). Ordenadas e sem repetição.
pub fn script_vars(script: &Script, all: &[Script]) -> Vec<String> {
    fn walk<'a>(blocks: &'a [Block], all: &'a [Script], seen: &mut Vec<&'a str>, out: &mut std::collections::BTreeSet<String>) {
        for b in blocks {
            out.extend(block_vars(b).into_iter().filter(|n| !n.trim().is_empty()).map(String::from));
            if let Block::Call { script } = b {
                if !seen.contains(&script.as_str()) {
                    seen.push(script);
                    if let Some(s) = all.iter().find(|s| s.name == *script) {
                        walk(&s.blocks, all, seen, out);
                    }
                }
            }
            for c in children(b) {
                walk(c, all, seen, out);
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    // Variáveis de resultado das integrações (ticket 09): r, r_ok, r_status e r_pendente.
    for r in script.integrations.iter().filter_map(|i| i.result.as_deref()).filter(|r| !r.trim().is_empty()) {
        for suf in ["", "_ok", "_status", "_pendente"] {
            out.insert(format!("{}{suf}", r.trim()));
        }
    }
    walk(&script.blocks, all, &mut vec![script.name.as_str()], &mut out);
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CondKind, Condition, Region};

    fn q(text: &str, cursor: usize, kind: FieldKind) -> Option<Query> {
        query(text, cursor, kind)
    }

    #[test]
    fn every_example_runs() {
        for d in FUNCTIONS {
            let mut ctx = crate::runner::new_context();
            assert!(evalexpr::eval_with_context_mut(d.example, &mut ctx).is_ok(), "exemplo de {} falhou: {}", d.name, d.example);
            assert!(d.example.contains(d.name), "o exemplo de {} não usa a função", d.name);
            assert_eq!(model::expr_error(d.example), None, "{}", d.example);
        }
        for op in OPERATORS {
            let mut ctx = crate::runner::new_context();
            assert!(evalexpr::eval_with_context_mut(op.example, &mut ctx).is_ok(), "exemplo de {} falhou: {}", op.op, op.example);
        }
    }

    #[test]
    fn registered_functions_are_cataloged() {
        for (name, _) in crate::runner::custom_functions() {
            assert!(FUNCTIONS.iter().any(|d| d.name == name && d.ours), "função {name} registrada pelo UltraTask falta no catálogo (ou não está marcada ours)");
        }
        let ours = FUNCTIONS.iter().filter(|d| d.ours).count();
        assert_eq!(ours, crate::runner::custom_functions().len(), "o catálogo marca como do UltraTask uma função que não é registrada");
    }

    #[test]
    fn skill_mentions_every_function() {
        let skill = include_str!("../skills/ultratask-scripts/SKILL.md");
        for d in FUNCTIONS {
            assert!(skill.contains(&format!("`{}", d.name)), "a skill não cita a função {}", d.name);
        }
    }

    #[test]
    fn suggests_rand_and_inserts() {
        let qr = q("x + ra", 6, FieldKind::Expr).unwrap();
        assert_eq!((qr.prefix.as_str(), qr.word_start), ("ra", 4));
        let s = suggestions(&qr, &[]);
        assert_eq!(s[0].label, "rand(a, b)");
        assert!(!s[0].desc.is_empty());
        assert_eq!(apply("x + ra", qr.word_start, qr.cursor, &s[0].insert), ("x + rand(".into(), 9));
        // no meio do texto: o resto fica depois do cursor
        assert_eq!(apply("ra + 1", 0, 2, "rand("), ("rand( + 1".into(), 5));
    }

    #[test]
    fn filter_rules() {
        let vars = vec!["contador".to_string(), "texto".to_string()];
        let names = |text: &str| suggestions(&q(text, text.chars().count(), FieldKind::Expr).unwrap(), &vars).into_iter().map(|s| s.insert).collect::<Vec<_>>();
        assert_eq!(names("con"), ["contador", "contem("]);
        assert_eq!(names("CON"), ["contador", "contem("]);
        assert_eq!(names("trim"), ["str::trim("]);
        assert_eq!(names("str::to_u"), ["str::to_uppercase("]);
        assert!(names("12").is_empty());
        assert!(names("contem(\"te").is_empty(), "dentro de aspas não sugere");
        assert_eq!(names("contador"), Vec::<String>::new(), "variável já digitada inteira some");
        assert_eq!(names("").len(), vars.len() + FUNCTIONS.len());
    }

    #[test]
    fn braces() {
        assert_eq!(brace_span("oi {ra} x", 5), Some((4, 6)));
        assert_eq!(brace_span("oi {ra} x", 4), Some((4, 6)));
        assert_eq!(brace_span("oi {ra} x", 6), Some((4, 6)));
        assert_eq!(brace_span("oi {ra} x", 7), None);
        assert_eq!(brace_span("oi {ra} x", 2), None);
        assert_eq!(brace_span("{{ra}} {x", 3), None, "chaves duplas são literais");
        assert_eq!(brace_span("{{ra}} {x", 9), Some((8, 9)), "chave aberta vale até o fim");
        assert_eq!(brace_span("é {á}", 3), Some((3, 4)), "índices em caracteres");

        let t = "n = {ra} fim";
        let qr = q(t, 7, FieldKind::Text).unwrap();
        assert_eq!((qr.expr.as_str(), qr.prefix.as_str()), ("ra", "ra"));
        assert_eq!(suggestions(&qr, &[])[0].insert, "rand(");
        assert_eq!(q(t, 2, FieldKind::Text), None, "fora das chaves a cola não aparece");
        assert_eq!(q(t, 10, FieldKind::Text), None);
    }

    #[test]
    fn query_reports_errors() {
        assert!(q("x >", 3, FieldKind::Expr).unwrap().error().is_some());
        assert_eq!(q("x > 1", 5, FieldKind::Expr).unwrap().error(), None);
        assert!(q("a {x +} b", 5, FieldKind::Text).unwrap().error().is_some());
    }

    #[test]
    fn vars_from_every_level_and_calls() {
        let s = |v: &str| v.to_string();
        let sub = Script { name: s("sub"), blocks: vec![Block::SetVar { name: s("do_sub"), expr: s("1") }, Block::Call { script: s("main") }], ..Default::default() };
        let main = Script {
            name: s("main"),
            blocks: vec![
                Block::SetVar { name: s("b"), expr: s("1") },
                Block::If {
                    cond: Condition { kind: CondKind::Expr(s("true")), negate: false },
                    then: vec![Block::Repeat { times: s("2"), body: vec![Block::OcrToVar { name: s("lido"), region: Region::default() }] }],
                    else_: vec![Block::While { cond: Condition::default(), body: vec![Block::SetVar { name: s("a"), expr: s("2") }, Block::SetVar { name: s("b"), expr: s("3") }] }],
                },
                Block::Call { script: s("sub") },
            ],
            ..Default::default()
        };
        let all = vec![main.clone(), sub];
        assert_eq!(script_vars(&main, &all), ["a", "b", "do_sub", "lido"]);
    }

    #[test]
    fn uia_read_vars_are_listed() {
        let uia = |action| Block::Uia { elem: Default::default(), action, timeout_ms: "1".into(), fallback_click: false };
        let sc = Script { blocks: vec![uia(UiaAction::ReadValue("valor".into())), uia(UiaAction::ReadName("nome".into())), uia(UiaAction::Invoke)], ..Default::default() };
        assert_eq!(script_vars(&sc, &[]), ["nome", "valor"]);
    }

    #[test]
    fn integration_result_vars_are_listed() {
        let mut sc = Script::default();
        sc.integrations.push(crate::integrations::Integration { result: Some("endereco".into()), ..Default::default() });
        let v = script_vars(&sc, &[]);
        for n in ["endereco", "endereco_ok", "endereco_status", "endereco_pendente"] {
            assert!(v.iter().any(|x| x == n), "{n} faltou em {v:?}");
        }
    }
}
