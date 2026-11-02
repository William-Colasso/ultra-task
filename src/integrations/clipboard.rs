//! Destino Área de transferência: escreve o envio como texto, ou lê o texto atual para `r`.
//! `r_status` recebe o número de caracteres escritos ou lidos.
use super::{Body, Destination, Job, Outcome};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum ClipMode {
    /// Põe o envio na área de transferência.
    #[default]
    Write,
    /// Lê o texto da área de transferência para a variável de resultado; o envio é ignorado.
    Read,
}

impl ClipMode {
    pub const ALL: [ClipMode; 2] = [ClipMode::Write, ClipMode::Read];
    pub fn label(self) -> &'static str {
        match self {
            ClipMode::Write => "Escrever",
            ClipMode::Read => "Ler",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ClipboardDest {
    pub mode: ClipMode,
}

impl Destination for ClipboardDest {
    fn kind(&self) -> &'static str {
        "Clipboard"
    }

    fn validate(&self, _path: &str, _errs: &mut Vec<String>) {}

    fn prepare(&self, body: Body, _eval: &mut dyn FnMut(&str) -> Result<String, String>) -> Result<Job, String> {
        let mode = self.mode;
        Ok(Box::new(move || {
            let r = match mode {
                ClipMode::Write => write(&body.text).map(|n| (n, String::new())),
                ClipMode::Read => read().map(|t| (t.chars().count(), t)),
            };
            match r {
                Ok((n, text)) => Outcome { ok: true, status: n as i64, body: text, error: None },
                Err(e) => Outcome::failed(format!("área de transferência: {e}")),
            }
        }))
    }

    fn timeout_ms(&self) -> u64 {
        5000
    }
}

/// Abre a área de transferência; outro programa pode estar com ela aberta, então tenta por ~1 s.
struct Open;

impl Open {
    fn new() -> Result<Open, String> {
        for _ in 0..50 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Open);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("ocupada por outro programa".into())
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Escreve `text` como texto Unicode. Devolve o número de caracteres.
pub fn write(text: &str) -> Result<usize, String> {
    let w: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let _open = Open::new()?;
    unsafe {
        EmptyClipboard().map_err(|e| e.message())?;
        let mem = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2).map_err(|e| e.message())?;
        let p = GlobalLock(mem) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(mem));
            return Err("sem memória".into());
        }
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        let _ = GlobalUnlock(mem);
        if let Err(e) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0))) {
            let _ = GlobalFree(Some(mem)); // só é do sistema quando dá certo
            return Err(e.message());
        }
    }
    Ok(text.chars().count())
}

/// Lê o texto atual; vazio quando não há texto.
pub fn read() -> Result<String, String> {
    let _open = Open::new()?;
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return Ok(String::new());
        }
        let h = GetClipboardData(CF_UNICODETEXT.0 as u32).map_err(|e| e.message())?;
        let mem = HGLOBAL(h.0);
        let p = GlobalLock(mem) as *const u16;
        if p.is_null() {
            return Ok(String::new());
        }
        let max = GlobalSize(mem) / 2;
        let len = (0..max).take_while(|&i| *p.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        let _ = GlobalUnlock(mem);
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::tests::{run as run_flow, set, var, wait_until};
    use crate::integrations::{Dest, Integration, Payload, When};

    #[test]
    fn json_contract() {
        let d: Dest = serde_json::from_str(r#"{"Clipboard": {"mode": "Read"}}"#).unwrap();
        assert_eq!(d, Dest::Clipboard(ClipboardDest { mode: ClipMode::Read }));
        assert_eq!(serde_json::from_str::<ClipboardDest>("{}").unwrap(), ClipboardDest { mode: ClipMode::Write });
        assert!(serde_json::from_str::<ClipboardDest>(r#"{"mode": "Colar"}"#).is_err());
        assert!(serde_json::from_str::<ClipboardDest>(r#"{"mode": "Read", "x": 1}"#).is_err());
    }

    /// Ida e volta pela área de transferência real, pelo fluxo: escreve com acento e quebra de
    /// linha, lê de volta para uma variável. Restaura o conteúdo anterior no fim.
    #[test]
    #[ignore = "mexe na área de transferência real; rode no Windows com --include-ignored"]
    fn clipboard_round_trip() {
        let before = read().unwrap_or_default();
        let list = vec![
            Integration { name: "copia".into(), when: When::OnSet(vec!["n".into()]), dest: Dest::Clipboard(ClipboardDest { mode: ClipMode::Write }), payload: Payload::Text("Pedido nº {n}\r\nçã ok".into()), result: Some("w".into()) },
            Integration { name: "cola".into(), when: When::OnSet(vec!["ler".into()]), dest: Dest::Clipboard(ClipboardDest { mode: ClipMode::Read }), payload: Payload::Text(String::new()), result: Some("c".into()) },
        ];
        let blocks = vec![set("n", "7"), wait_until("!w_pendente", 5000), set("ler", "1"), wait_until("!c_pendente", 5000)];
        let r = run_flow(list, blocks, vec![]);
        let _ = write(&before);
        assert_eq!(r.result, Ok(()), "{:?}", r.logs);
        assert!(r.logs.is_empty(), "{:?}", r.logs);
        assert_eq!((var(&r, "w_ok"), var(&r, "w_status")), ("true", "18"));
        assert_eq!((var(&r, "c"), var(&r, "c_ok"), var(&r, "c_status")), ("\"Pedido nº 7\r\nçã ok\"", "true", "18"));
    }
}
