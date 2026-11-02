//! Destino Comando: roda um programa sem janela de console.
//!
//! - O envio vai pelo stdin; o stdout, sem espaços nas pontas, vai para `r`; o código de saída
//!   vai para `r_status` (`r_ok` é verdadeiro só com 0).
//! - Se o timeout estoura, o processo e os filhos dele (num job object) são encerrados.
//! - A pasta de trabalho é a do exe.
use super::{Body, Destination, Job, Outcome, check_interp};
use crate::model::interpolate;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CommandDest {
    /// Programa (caminho ou nome no PATH); aceita `{expressão}`.
    pub program: String,
    /// Um argumento por item; cada um aceita `{expressão}`.
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl Default for CommandDest {
    fn default() -> Self {
        CommandDest { program: "cmd".into(), args: vec!["/c".into(), "echo ok".into()], timeout_ms: 10000 }
    }
}

impl Destination for CommandDest {
    fn kind(&self) -> &'static str {
        "Command"
    }

    fn validate(&self, path: &str, errs: &mut Vec<String>) {
        if self.program.trim().is_empty() {
            errs.push(format!("{path}.program: vazio"));
        }
        check_interp(errs, &format!("{path}.program"), &self.program);
        for (k, a) in self.args.iter().enumerate() {
            check_interp(errs, &format!("{path}.args[{k}]"), a);
        }
        if self.timeout_ms == 0 {
            errs.push(format!("{path}.timeout_ms: deve ser > 0"));
        }
    }

    fn prepare(&self, body: Body, eval: &mut dyn FnMut(&str) -> Result<String, String>) -> Result<Job, String> {
        let program = interpolate(&self.program, &mut *eval)?.trim().to_string();
        if program.is_empty() {
            return Err("o programa ficou vazio".into());
        }
        let mut args = vec![];
        for a in &self.args {
            args.push(interpolate(a, &mut *eval)?);
        }
        let timeout = self.timeout_ms;
        Ok(Box::new(move || run(&program, &args, body.text, timeout)))
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

/// Job object que permite encerrar o processo junto com os filhos que ele abriu.
struct ProcJob(HANDLE);

impl ProcJob {
    fn attach(child: &std::process::Child) -> Option<ProcJob> {
        unsafe {
            let job = CreateJobObjectW(None, None).ok()?;
            let job = ProcJob(job);
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())).ok()?;
            Some(job)
        }
    }
}

impl Drop for ProcJob {
    fn drop(&mut self) {
        // Sem KILL_ON_JOB_CLOSE: o que o comando abriu de propósito (ex.: `start notepad`) continua.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Texto da saída: UTF-8 quando for válido; senão, a página de código OEM (a do console).
pub fn decode(bytes: &[u8]) -> String {
    use windows::Win32::Globalization::{CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut w = vec![0u16; n as usize];
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut w));
        String::from_utf16_lossy(&w[..n.max(0) as usize])
    }
}

fn reader(mut r: impl Read + Send + 'static) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut buf = vec![];
        let _ = r.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    rx
}

/// Roda o programa (na thread da integração) e espera no máximo `timeout_ms`.
pub fn run(program: &str, args: &[String], input: String, timeout_ms: u64) -> Outcome {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(crate::model::base_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW.0);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Outcome::failed(format!("programa \"{program}\" não encontrado")),
        Err(e) => return Outcome::failed(format!("programa \"{program}\": {e}")),
    };
    let job = ProcJob::attach(&child);
    let t0 = Instant::now();
    let limit = Duration::from_millis(timeout_ms);
    if let Some(mut stdin) = child.stdin.take() {
        // Numa thread: um envio grande não trava enquanto o programa não lê.
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let out = reader(child.stdout.take().expect("stdout"));
    let err = reader(child.stderr.take().expect("stderr"));
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(-1),
            Ok(None) if t0.elapsed() < limit => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                match &job {
                    Some(j) => unsafe {
                        let _ = TerminateJobObject(j.0, 1);
                    },
                    None => {
                        let _ = child.kill();
                    }
                }
                let _ = child.wait();
                return Outcome::failed(format!("tempo esgotado ({timeout_ms} ms): o processo \"{program}\" foi encerrado"));
            }
            Err(e) => return Outcome::failed(format!("programa \"{program}\": {e}")),
        }
    };
    // Um filho em segundo plano pode segurar a saída aberta: espera o que resta do timeout.
    let rest = limit.saturating_sub(t0.elapsed()).max(Duration::from_millis(500));
    let Ok(stdout) = out.recv_timeout(rest) else {
        return Outcome { ok: false, status: code as i64, body: String::new(), error: Some(format!("o processo \"{program}\" terminou, mas a saída continuou aberta (um filho em segundo plano?)")) };
    };
    let stderr = err.recv_timeout(Duration::from_millis(200)).unwrap_or_default();
    let body = decode(&stdout).trim().to_string();
    let error = (code != 0).then(|| {
        let e = decode(&stderr);
        let e: String = e.trim().chars().take(200).collect();
        if e.is_empty() { format!("\"{program}\" saiu com código {code}") } else { format!("\"{program}\" saiu com código {code}: {e}") }
    });
    Outcome { ok: code == 0, status: code as i64, body, error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::{run as run_flow, set, var, wait_until};
    use crate::integrations::{Dest, Integration, Payload, When};

    fn cmd(args: &[&str], timeout_ms: u64) -> Dest {
        Dest::Command(CommandDest { program: "cmd".into(), args: args.iter().map(|s| s.to_string()).collect(), timeout_ms })
    }

    #[test]
    fn json_contract_and_validation() {
        let it: Integration = serde_json::from_str(
            r#"{"name": "c", "when": {"OnSet": ["n"]}, "dest": {"Command": {"program": "cmd", "args": ["/c", "echo {n}"], "timeout_ms": 5000}}, "payload": {"Text": "{n}"}, "result": "r"}"#,
        )
        .unwrap();
        assert_eq!(it.dest, cmd(&["/c", "echo {n}"], 5000));
        assert_eq!(serde_json::from_str::<CommandDest>("{}").unwrap(), CommandDest::default());
        assert!(serde_json::from_str::<CommandDest>(r#"{"program": "a", "cwd": "x"}"#).is_err());
        let mut errs = vec![];
        CommandDest { program: "".into(), args: vec!["ok".into(), "{x +}".into()], timeout_ms: 0 }.validate("d", &mut errs);
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(errs[0] == "d.program: vazio" && errs[1].starts_with("d.args[1]: {x +}") && errs[2] == "d.timeout_ms: deve ser > 0", "{errs:?}");
    }

    /// Critério do ticket: com `cmd /c`, o stdout (sem espaços nas pontas) chega na variável, o
    /// envio entra pelo stdin, e o código de saída chega em `r_status`.
    #[test]
    fn cmd_stdout_stdin_and_exit_code() {
        let list = vec![
            Integration { name: "eco".into(), when: When::OnSet(vec!["n".into()]), dest: cmd(&["/c", "echo   valor {n * 2}  "], 5000), payload: Payload::Text(String::new()), result: Some("r".into()) },
            Integration { name: "entrada".into(), when: When::OnSet(vec!["n".into()]), dest: cmd(&["/c", "findstr pedido"], 5000), payload: Payload::Text("outra linha\r\npedido {n}\r\n".into()), result: Some("e".into()) },
            Integration { name: "sai7".into(), when: When::OnSet(vec!["n".into()]), dest: cmd(&["/c", "echo parcial& exit 7"], 5000), payload: Payload::Text(String::new()), result: Some("x".into()) },
        ];
        let blocks = vec![set("n", "21"), wait_until("!r_pendente && !e_pendente && !x_pendente", 8000)];
        let r = run_flow(list, blocks, vec![]);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert_eq!((var(&r, "r"), var(&r, "r_ok"), var(&r, "r_status")), ("\"valor 42\"", "true", "0"), "{:?}", r.logs);
        assert_eq!((var(&r, "e"), var(&r, "e_ok")), ("\"pedido 21\"", "true"), "{:?}", r.logs);
        assert_eq!((var(&r, "x"), var(&r, "x_ok"), var(&r, "x_status")), ("\"parcial\"", "false", "7"));
        assert_eq!(r.logs, ["integração 'sai7': \"cmd\" saiu com código 7"]);
    }

    /// Um comando que trava é encerrado no timeout: o fluxo segue, o log diz o motivo e o
    /// processo para de fato (o arquivo em que ele escreve sem parar deixa de crescer).
    #[test]
    fn hung_command_is_killed_on_timeout() {
        // Caminho relativo (a pasta de trabalho é a do exe): sem aspas, que o cmd e o escape do
        // Rust tratam de jeitos diferentes.
        let nome = format!("ultratask-laco-{}.txt", std::process::id());
        let f = crate::model::base_dir().join(&nome);
        let _ = std::fs::remove_file(&f);
        let laco = format!("for /l %i in (0,0,1) do @echo x>>{nome}");
        let t0 = Instant::now();
        let o = run("cmd", &["/c".into(), laco], String::new(), 700);
        let took = t0.elapsed();
        assert!(!o.ok && o.status == 0, "{o:?}");
        assert!(o.error.as_deref().unwrap_or("").starts_with("tempo esgotado (700 ms)"), "{o:?}");
        assert!(took >= Duration::from_millis(700) && took < Duration::from_millis(2500), "{took:?}");
        let size = || std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        let a = size();
        assert!(a > 0, "o laço nem chegou a rodar");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(size(), a, "o processo continuou rodando depois do timeout");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn missing_program_goes_to_log() {
        let o = run("programa_que_nao_existe_ultratask", &[], String::new(), 1000);
        assert_eq!((o.ok, o.status), (false, 0));
        assert_eq!(o.error.as_deref(), Some("programa \"programa_que_nao_existe_ultratask\" não encontrado"));
    }

    #[test]
    fn decode_utf8_or_oem() {
        assert_eq!(decode("olá".as_bytes()), "olá");
        // 0x82 é "é" na página 850 e na 437 (as OEM mais comuns); no Wine depende do locale.
        assert!(!decode(b"caf\x82").is_empty());
    }
}
