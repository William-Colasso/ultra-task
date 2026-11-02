//! Destino Arquivo: acrescenta, sobrescreve ou lê um arquivo de texto.
//!
//! - Caminho com `{…}`; relativo parte da pasta do exe; as pastas que faltam são criadas.
//! - Ao gravar, JSON vira uma linha por envio, CSV ganha a linha de cabeçalho quando o
//!   arquivo é novo (ou vazio), e texto vai como está.
//! - `r` recebe o conteúdo lido (modo Ler) e `r_status` o número de bytes gravados ou lidos.
use super::{Body, Destination, Job, Outcome, check_interp, eval_text};
use evalexpr::HashMapContext;
use crate::model::interpolate;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum FileMode {
    /// Grava no fim do arquivo.
    #[default]
    Append,
    /// Troca o conteúdo do arquivo pelo envio.
    Overwrite,
    /// Lê o arquivo inteiro para a variável de resultado; o envio é ignorado.
    Read,
}

impl FileMode {
    pub const ALL: [FileMode; 3] = [FileMode::Append, FileMode::Overwrite, FileMode::Read];
    pub fn label(self) -> &'static str {
        match self {
            FileMode::Append => "Acrescentar",
            FileMode::Overwrite => "Sobrescrever",
            FileMode::Read => "Ler",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FileDest {
    /// Aceita `{expressão}`; relativo à pasta do exe.
    pub path: String,
    pub mode: FileMode,
}

impl Default for FileDest {
    fn default() -> Self {
        FileDest { path: "saida.txt".into(), mode: FileMode::Append }
    }
}

/// Caminho absoluto: um relativo parte da pasta do exe.
pub fn resolve(path: &str) -> PathBuf {
    let p = Path::new(path.trim());
    if p.is_absolute() { p.to_path_buf() } else { crate::model::base_dir().join(p) }
}

/// Mensagem de erro de E/S em pt-BR, com o caminho.
pub fn io_error(path: &Path, e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    let what = match e.kind() {
        K::NotFound => "não encontrado".to_string(),
        K::PermissionDenied => "sem permissão".to_string(),
        // ERROR_INVALID_NAME (123) e ERROR_BAD_PATHNAME (161).
        K::InvalidFilename => "caminho inválido".to_string(),
        _ if matches!(e.raw_os_error(), Some(123 | 161)) => "caminho inválido".to_string(),
        _ => e.to_string(),
    };
    format!("arquivo \"{}\": {what}", path.display())
}

impl Destination for FileDest {
    fn kind(&self) -> &'static str {
        "File"
    }

    fn validate(&self, path: &str, errs: &mut Vec<String>) {
        if self.path.trim().is_empty() {
            errs.push(format!("{path}.path: vazio"));
        }
        check_interp(errs, &format!("{path}.path"), &self.path);
    }

    fn prepare(&self, body: Body, vars: &HashMapContext) -> Result<Job, String> {
        let eval = &mut |e: &str| eval_text(vars, e);
        let p = interpolate(&self.path, eval)?;
        if p.trim().is_empty() {
            return Err("o caminho do arquivo ficou vazio".into());
        }
        let (path, mode) = (resolve(&p), self.mode);
        Ok(Box::new(move || run(&path, mode, &body)))
    }

    fn timeout_ms(&self) -> u64 {
        10_000
    }
}

/// Faz a gravação ou a leitura (roda na thread da integração).
pub fn run(path: &Path, mode: FileMode, body: &Body) -> Outcome {
    match exec(path, mode, body) {
        Ok((n, text)) => Outcome { ok: true, status: n as i64, body: text, error: None },
        Err(e) => Outcome::failed(io_error(path, &e)),
    }
}

fn exec(path: &Path, mode: FileMode, body: &Body) -> std::io::Result<(usize, String)> {
    if mode == FileMode::Read {
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned();
        return Ok((bytes.len(), text));
    }
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    let append = mode == FileMode::Append;
    let mut f = std::fs::OpenOptions::new().create(true).append(append).write(true).truncate(!append).open(path)?;
    // Só um arquivo novo (ou vazio) ganha o cabeçalho do CSV.
    let fresh = f.metadata()?.len() == 0;
    let mut out = String::new();
    match &body.header {
        Some(h) if fresh => out += &format!("{h}\r\n{}\r\n", body.text),
        Some(_) => out += &format!("{}\r\n", body.text),
        None if body.content_type.starts_with("application/json") => out += &format!("{}\r\n", body.text),
        None => out += &body.text,
    }
    f.write_all(out.as_bytes())?;
    Ok((out.len(), String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::{run as run_flow, set, var, wait_until};
    use crate::integrations::{Dest, Integration, Payload, When};

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ultratask-arquivo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn integ(name: &str, on: &str, path: &Path, mode: FileMode, payload: Payload, result: Option<&str>) -> Integration {
        Integration {
            name: name.into(),
            when: When::OnSet(vec![on.into()]),
            dest: Dest::File(FileDest { path: path.display().to_string(), mode }),
            payload,
            result: result.map(Into::into),
        }
    }

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn relative_path_starts_at_exe_folder() {
        assert_eq!(resolve("dados/x.txt"), crate::model::base_dir().join("dados/x.txt"));
        let abs = std::env::temp_dir().join("y.txt");
        assert_eq!(resolve(&abs.display().to_string()), abs);
    }

    #[test]
    fn json_contract_and_validation() {
        let it: Integration = serde_json::from_str(
            r#"{"name": "log", "when": {"OnSet": ["n"]}, "dest": {"File": {"path": "saida/{dia}.csv", "mode": "Overwrite"}}, "payload": {"Csv": ["n"]}, "result": null}"#,
        )
        .unwrap();
        assert_eq!(it.dest, Dest::File(FileDest { path: "saida/{dia}.csv".into(), mode: FileMode::Overwrite }));
        let d: FileDest = serde_json::from_str("{}").unwrap();
        assert_eq!(d, FileDest::default());
        assert!(serde_json::from_str::<FileDest>(r#"{"path": "a", "x": 1}"#).is_err());
        assert!(serde_json::from_str::<FileDest>(r#"{"mode": "Apagar"}"#).is_err());
        let mut errs = vec![];
        FileDest { path: " ".into(), mode: FileMode::Read }.validate("d", &mut errs);
        FileDest { path: "a/{x +}.txt".into(), mode: FileMode::Read }.validate("d", &mut errs);
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs[0].starts_with("d.path: vazio") && errs[1].starts_with("d.path: {x +}"), "{errs:?}");
    }

    /// Acrescentar (JSON uma linha por envio, CSV com cabeçalho uma vez só, texto como está),
    /// sobrescrever e ler de volta para uma variável, tudo pelo fluxo.
    #[test]
    fn append_overwrite_csv_header_and_read() {
        let d = tmp("modos");
        let (json, csv, txt, over) = (d.join("sub/pasta/log.jsonl"), d.join("tabela.csv"), d.join("t.txt"), d.join("ultimo.txt"));
        let list = vec![
            integ("json", "n", &json, FileMode::Append, Payload::Json(names(&["n", "nome"])), Some("rj")),
            integ("csv", "n", &csv, FileMode::Append, Payload::Csv(names(&["n", "nome"])), None),
            integ("txt", "n", &txt, FileMode::Append, Payload::Text("[{n}]".into()), None),
            integ("sobre", "n", &over, FileMode::Overwrite, Payload::Text("n={n}".into()), None),
            integ("ler", "le", &csv, FileMode::Read, Payload::Json(vec![]), Some("conteudo")),
        ];
        let mut blocks = vec![set("nome", r#""Zé, o \"bom\"""#)];
        for k in 1..=3 {
            blocks.push(set("n", &k.to_string()));
            blocks.push(wait_until("!rj_pendente", 5000));
            // As outras não têm variável de resultado: dá um tempo para gravarem em ordem.
            blocks.push(crate::model::Block::Wait { ms: "150".into(), jitter_ms: 0 });
        }
        blocks.push(set("le", "1"));
        blocks.push(wait_until("!conteudo_pendente", 5000));
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert!(r.logs.is_empty(), "{:?}", r.logs);
        let read = |p: &Path| std::fs::read_to_string(p).unwrap();
        assert_eq!(read(&json), "{\"n\":1,\"nome\":\"Zé, o \\\"bom\\\"\"}\r\n{\"n\":2,\"nome\":\"Zé, o \\\"bom\\\"\"}\r\n{\"n\":3,\"nome\":\"Zé, o \\\"bom\\\"\"}\r\n");
        let linhas = "n,nome\r\n1,\"Zé, o \"\"bom\"\"\"\r\n2,\"Zé, o \"\"bom\"\"\"\r\n3,\"Zé, o \"\"bom\"\"\"\r\n";
        assert_eq!(read(&csv), linhas);
        assert_eq!(read(&txt), "[1][2][3]");
        assert_eq!(read(&over), "n=3");
        assert_eq!(var(&r, "rj_ok"), "true");
        assert_eq!(var(&r, "conteudo"), format!("\"{linhas}\""));
        assert_eq!(var(&r, "conteudo_ok"), "true");
        assert_eq!(var(&r, "conteudo_status"), linhas.len().to_string());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Um CSV que já existe com conteúdo não ganha outro cabeçalho; sobrescrever um CSV escreve
    /// cabeçalho + linha.
    #[test]
    fn csv_header_only_once() {
        let d = tmp("csv");
        let p = d.join("a.csv");
        let vars = crate::integrations::tests::ctx(&[("a", evalexpr::Value::Int(1)), ("b", evalexpr::Value::String("x".into()))]);
        let body = Payload::Csv(names(&["a", "b"])).render(&vars).unwrap();
        for _ in 0..2 {
            assert!(run(&p, FileMode::Append, &body).ok);
        }
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a,b\r\n1,x\r\n1,x\r\n");
        let o = run(&p, FileMode::Overwrite, &body);
        assert!(o.ok && o.status == 10, "{o:?}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a,b\r\n1,x\r\n");
        let o = run(&p, FileMode::Read, &body);
        assert_eq!((o.ok, o.body.as_str()), (true, "a,b\r\n1,x\r\n"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Caminho inválido, arquivo inexistente e gravar "em cima" de uma pasta: vão para o log e
    /// deixam `r_ok` falso.
    #[test]
    fn errors_go_to_log_and_r_ok_false() {
        let d = tmp("erros");
        std::fs::create_dir_all(d.join("uma_pasta")).unwrap();
        let list = vec![
            integ("invalido", "a", Path::new("C:\\nome<invalido>|?.txt"), FileMode::Append, Payload::Text("x".into()), Some("r1")),
            integ("sem_arquivo", "a", &d.join("nao_existe.txt"), FileMode::Read, Payload::Text("x".into()), Some("r2")),
            integ("pasta", "a", &d.join("uma_pasta"), FileMode::Overwrite, Payload::Text("x".into()), Some("r3")),
        ];
        let blocks = vec![set("a", "1"), wait_until("!r1_pendente && !r2_pendente && !r3_pendente", 5000)];
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()));
        for k in 1..=3 {
            assert_eq!(var(&r, &format!("r{k}_ok")), "false", "r{k}: {:?}", r.logs);
            assert_eq!(var(&r, &format!("r{k}_status")), "0");
        }
        assert_eq!(r.logs.len(), 3, "{:?}", r.logs);
        // As entregas rodam em threads: a ordem do log não é fixa.
        assert!(r.logs.iter().any(|l| l == "integração 'invalido': arquivo \"C:\\nome<invalido>|?.txt\": caminho inválido"), "{:?}", r.logs);
        assert!(r.logs.iter().any(|l| l.starts_with("integração 'sem_arquivo'") && l.ends_with("não encontrado")), "{:?}", r.logs);
        assert!(r.logs.iter().any(|l| l.starts_with("integração 'pasta'") && l.contains("uma_pasta") && l.ends_with("sem permissão")), "{:?}", r.logs);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Arquivo somente leitura: gravar falha com erro de permissão.
    #[test]
    #[ignore = "o Wine rodando como root ignora o atributo somente leitura; rode no Windows"]
    fn read_only_file_is_permission_error() {
        let d = tmp("ro");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("ro.txt");
        std::fs::write(&p, "fixo").unwrap();
        let mut perm = std::fs::metadata(&p).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&p, perm.clone()).unwrap();
        let body = Body { content_type: "text/plain; charset=utf-8", text: "x".into(), header: None };
        let o = run(&p, FileMode::Append, &body);
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&p, perm).unwrap();
        assert!(!o.ok && o.status == 0, "{o:?}");
        assert!(o.error.as_deref().unwrap_or("").ends_with("sem permissão"), "{o:?}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "fixo");
        let _ = std::fs::remove_dir_all(&d);
    }
}
