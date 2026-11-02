//! Exportar um script como .exe que só executa (ticket 05).
//!
//! O .exe exportado é uma cópia do próprio UltraTask com um *payload* anexado ao fim do
//! arquivo: `[JSON do Payload][tamanho do JSON: u64 LE][MAGIC]`. O Windows ignora bytes
//! depois das seções do PE, então a cópia roda normalmente; ao iniciar, `main` procura o
//! rodapé e, se achar, roda o [`run_player`] em vez do editor.
//!
//! O player não lê nem grava a pasta de scripts nem os ajustes do app, não usa a instância
//! única nem `RegisterHotKey` (o atalho de pânico é lido com `GetAsyncKeyState`), então não
//! conflita com o UltraTask aberto nem com outros .exe exportados.
use crate::model::{self, Block, Hotkey, Script, Settings, Trigger};
use crate::{hooks, runner, win};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Fecha o rodapé do payload. Não pode aparecer por acaso nos últimos bytes de um exe normal.
const MAGIC: &[u8; 8] = b"ULTRATK\x01";
const FOOTER: usize = 16;

/// Tudo o que o .exe exportado leva embutido.
///
/// **Integrações (ticket 09):** elas ficam no próprio `Script` (`integrations`), então já
/// viajam dentro de `script` e `called` pelo serde, sem mudança aqui. Se o ticket 09 guardar
/// algo fora do `Script` (ex.: segredos ou URLs base nos `Settings`), acrescente um campo
/// aqui com valor padrão e preencha em [`build_payload`]; o player passa esses dados ao
/// runner em [`run_player`] (veja `player_settings`).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Payload {
    pub script: Script,
    /// Scripts chamados com "Chamar script", em cadeia (sem o principal).
    pub called: Vec<Script>,
    pub panic: Hotkey,
    pub idle_resume_s: f32,
    pub ocr_scale: u32,
}

impl Default for Payload {
    fn default() -> Self {
        let s = Settings::default();
        Payload { script: Script::default(), called: vec![], panic: s.panic, idle_resume_s: s.idle_resume_s, ocr_scale: s.ocr_scale }
    }
}

// ---------- payload no fim do exe ----------

/// `exe` sem payload + o payload `p`.
pub fn append_payload(exe: &[u8], p: &Payload) -> Vec<u8> {
    let json = serde_json::to_vec(p).expect("Payload sempre serializa");
    let mut out = Vec::with_capacity(exe.len() + json.len() + FOOTER);
    out.extend_from_slice(exe);
    out.extend_from_slice(&json);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// Lê o rodapé: `None` = não há payload (exe normal); `Some(Err)` = rodapé com tamanho
/// impossível; `Some(Ok(n))` = os `n` bytes antes do rodapé são o JSON.
fn parse_footer(foot: &[u8], file_len: u64) -> Option<Result<u64, String>> {
    if foot.len() != FOOTER || &foot[8..] != MAGIC {
        return None;
    }
    let n = u64::from_le_bytes(foot[..8].try_into().unwrap());
    Some(if n == 0 || n > file_len - FOOTER as u64 { Err(format!("conteúdo embutido com tamanho inválido ({n} bytes)")) } else { Ok(n) })
}

fn decode(json: &[u8]) -> Result<Payload, String> {
    serde_json::from_slice(json).map_err(|e| format!("conteúdo embutido inválido: {e}"))
}

/// Separa o exe do payload. Sem payload, devolve os bytes inteiros e `None`.
pub fn split_payload(bytes: &[u8]) -> (&[u8], Option<Result<Payload, String>>) {
    if bytes.len() < FOOTER {
        return (bytes, None);
    }
    match parse_footer(&bytes[bytes.len() - FOOTER..], bytes.len() as u64) {
        None => (bytes, None),
        Some(Err(e)) => (bytes, Some(Err(e))),
        Some(Ok(n)) => {
            let start = bytes.len() - FOOTER - n as usize;
            (&bytes[..start], Some(decode(&bytes[start..bytes.len() - FOOTER])))
        }
    }
}

/// Como [`split_payload`], mas lendo só o fim do arquivo (o exe tem vários MB).
pub fn read_payload_file(path: &Path) -> Option<Result<Payload, String>> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < FOOTER as u64 {
        return None;
    }
    let mut foot = [0u8; FOOTER];
    f.seek(SeekFrom::End(-(FOOTER as i64))).ok()?;
    f.read_exact(&mut foot).ok()?;
    let n = match parse_footer(&foot, len)? {
        Ok(n) => n,
        Err(e) => return Some(Err(e)),
    };
    let mut json = vec![0u8; n as usize];
    let read = f.seek(SeekFrom::End(-((FOOTER as u64 + n) as i64))).and_then(|_| f.read_exact(&mut json));
    Some(read.map_err(|e| format!("conteúdo embutido ilegível: {e}")).and_then(|_| decode(&json)))
}

/// O payload do exe que está rodando; `None` = exe normal, abre o editor.
pub fn read_own_payload() -> Option<Result<Payload, String>> {
    read_payload_file(&std::env::current_exe().ok()?)
}

// ---------- montar o payload ----------

fn calls_in(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Call { script } => out.push(script.clone()),
            Block::If { then, else_, .. } => {
                calls_in(then, out);
                calls_in(else_, out);
            }
            Block::Repeat { body, .. } | Block::While { body, .. } => calls_in(body, out),
            _ => {}
        }
    }
}

/// Os scripts que `main` chama com "Chamar script", em cadeia e sem repetir, na ordem em
/// que aparecem. Nomes que não existem em `all` ficam de fora (o validador os aponta).
pub fn collect_chain(main: &Script, all: &[Script]) -> Vec<Script> {
    let mut out: Vec<Script> = vec![];
    let mut queue = vec![];
    calls_in(&main.blocks, &mut queue);
    let mut i = 0;
    while i < queue.len() {
        let name = queue[i].clone();
        i += 1;
        if name == main.name || out.iter().any(|s| s.name == name) {
            continue;
        }
        if let Some(s) = all.iter().find(|s| s.name == name) {
            calls_in(&s.blocks, &mut queue);
            out.push(s.clone());
        }
    }
    out
}

/// Valida e monta o conteúdo embutido. `all` = os scripts da biblioteca. Os erros dizem o
/// motivo da recusa, cada um prefixado com o nome do script.
pub fn build_payload(main: &Script, all: &[Script], settings: &Settings) -> Result<Payload, Vec<String>> {
    let mut errs = vec![];
    if main.target.is_none() {
        errs.push(format!("'{}': sem janela-alvo (defina na aba Config)", main.name));
    }
    if main.blocks.is_empty() {
        errs.push(format!("'{}': não tem blocos", main.name));
    }
    let known: Vec<String> = all.iter().map(|s| s.name.clone()).chain([main.name.clone()]).collect();
    let called = collect_chain(main, all);
    for (s, who) in std::iter::once((main, "")).chain(called.iter().map(|s| (s, " (chamado)"))) {
        errs.extend(model::validate(s, &known).into_iter().map(|e| format!("'{}'{who}: {e}", s.name)));
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    // O exportado ignora gatilho e atalho do script: nem os leva.
    let script = Script { hotkey: None, trigger: Trigger::Manual, ..main.clone() };
    Ok(Payload { script, called, panic: settings.panic, idle_resume_s: settings.idle_resume_s, ocr_scale: settings.ocr_scale })
}

pub fn default_exe_name(script: &str) -> String {
    let json = model::file_name(script);
    format!("{}.exe", json.strip_suffix(".json").unwrap_or(&json))
}

/// Grava em `dest` uma cópia do exe atual (sem payload antigo) com `p` embutido.
pub fn write_exported(dest: &Path, p: &Payload) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    if std::fs::canonicalize(dest).ok() == std::fs::canonicalize(&me).ok() && dest.exists() {
        return Err("não dá para sobrescrever o próprio UltraTask".into());
    }
    let bytes = std::fs::read(&me).map_err(|e| format!("ler {}: {e}", me.display()))?;
    let out = append_payload(split_payload(&bytes).0, p);
    let tmp = dest.with_extension("exe.tmp");
    std::fs::write(&tmp, &out).and_then(|_| std::fs::rename(&tmp, dest)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("gravar {}: {e}", dest.display())
    })
}

/// "📦 Exportar .exe": valida, abre "Salvar como" e grava. `Ok(None)` = cancelado.
pub fn export_interactive(main: &Script, all: &[Script], settings: &Settings) -> Result<Option<String>, String> {
    let p = build_payload(main, all, settings).map_err(|e| format!("Exportação recusada: {}", e.join("; ")))?;
    let Some(dest) = save_dialog(&default_exe_name(&main.name)) else { return Ok(None) };
    write_exported(&dest, &p).map_err(|e| format!("Erro ao exportar: {e}"))?;
    let n = p.called.len();
    let extra = if n == 0 { String::new() } else { format!(" (+{n} script(s) chamado(s))") };
    Ok(Some(format!("Exportado '{}'{extra} para {}", main.name, dest.display())))
}

fn save_dialog(default_name: &str) -> Option<PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::*;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    use windows::core::{PCWSTR, PWSTR};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let mut file = vec![0u16; 1024];
    for (d, s) in file.iter_mut().zip(default_name.encode_utf16().take(1000)) {
        *d = s;
    }
    let filter: Vec<u16> = "Executável (*.exe)\0*.exe\0\0".encode_utf16().collect();
    let (title, ext) = (wide("Exportar script como .exe"), wide("exe"));
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: unsafe { GetForegroundWindow() },
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if !unsafe { GetSaveFileNameW(&mut ofn) }.as_bool() {
        return None;
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}

// ---------- o .exe exportado ----------

/// O atalho está pressionado agora? `down(vk)` diz se uma tecla está apertada.
pub fn combo_down(hk: &Hotkey, down: impl Fn(u16) -> bool) -> bool {
    hk.vk != 0
        && down(hk.vk)
        && (!hk.ctrl || down(0x11))
        && (!hk.shift || down(0x10))
        && (!hk.alt || down(0x12))
        && (!hk.win || down(0x5B) || down(0x5C))
}

fn player_settings(p: &Payload) -> Settings {
    Settings { panic: p.panic, idle_resume_s: p.idle_resume_s, ocr_scale: p.ocr_scale, ..Settings::default() }
}

/// Roda o script embutido e devolve o código de saída: 0 ok, 1 erro, 2 parado.
pub fn run_player(p: Payload) -> i32 {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};

    // Aberto num terminal: o log vai para ele (o exe de release não tem console próprio).
    unsafe {
        let _ = windows::Win32::System::Console::AttachConsole(windows::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    let name = p.script.name.clone();
    let say = |m: &str| {
        let _ = writeln!(std::io::stdout(), "{m}");
    };
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (stop, hk) = (stop.clone(), p.panic);
        std::thread::spawn(move || {
            loop {
                if combo_down(&hk, |vk| unsafe { GetAsyncKeyState(vk as i32) } < 0) {
                    stop.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
    }
    let stop_item = MenuItem::new("Parar", true, None);
    let menu = Menu::new();
    let _ = menu.append(&stop_item);
    let tip = |t: &str| format!("UltraTask: {name} - {t}");
    let tray = Icon::from_rgba(crate::icon_rgba(), 32, 32)
        .ok()
        .and_then(|icon| TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(true).with_tooltip(tip("Iniciando")).with_icon(icon).build().ok());
    let set_tip = |t: &str| {
        if let Some(tr) = &tray {
            let _ = tr.set_tooltip(Some(tip(t)));
        }
    };
    // A bandeja precisa de mensagens processadas nesta thread.
    let pump = || {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if e.id == *stop_item.id() {
                stop.store(true, Ordering::SeqCst);
            }
        }
    };
    let Some(rule) = p.script.target.clone() else {
        say(&format!("[{name}] erro: o script não tem janela-alvo"));
        return 1;
    };
    // O modo Foco pausa quando você mexe no mouse/teclado; isso precisa dos hooks.
    let _hooks = (p.script.mode == model::Mode::Focus).then(|| hooks::Hooks::start(|| {}));
    let mut waiting = false;
    let target = loop {
        pump();
        if stop.load(Ordering::SeqCst) {
            say(&format!("[{name}] parado"));
            return 2;
        }
        if let Some(w) = win::find_matches(&rule).into_iter().next() {
            break w;
        }
        if !waiting {
            waiting = true;
            set_tip("Aguardando janela…");
            say(&format!("[{name}] aguardando janela {} / \"{}\"", rule.exe, rule.title));
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if !win::self_elevated() && win::is_elevated_pid(target.pid) {
        say(&format!("[{name}] erro: '{}' roda como administrador; rode este .exe como administrador também", target.title));
        return 1;
    }
    set_tip("Rodando");
    say(&format!("[{name}] janela-alvo: {}", target.label()));
    let (tx, rx) = std::sync::mpsc::channel();
    let all: Vec<Script> = std::iter::once(p.script.clone()).chain(p.called.iter().cloned()).collect();
    let h = runner::spawn(p.script.clone(), all, target, &player_settings(&p), 0, tx, Arc::new(|| {}));
    while !h.done.load(Ordering::SeqCst) {
        pump();
        while let Ok(l) = rx.try_recv() {
            say(&l);
        }
        if stop.load(Ordering::SeqCst) {
            h.stop.store(true, Ordering::SeqCst);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    while let Ok(l) = rx.try_recv() {
        say(&l);
    }
    h.exit_code.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TargetRule, validate};

    fn script(name: &str, blocks: Vec<Block>) -> Script {
        Script { name: name.into(), target: Some(TargetRule { exe: "notepad.exe".into(), title: "*".into() }), blocks, ..Default::default() }
    }
    fn call(s: &str) -> Block {
        Block::Call { script: s.into() }
    }

    #[test]
    fn payload_roundtrip() {
        let exe = b"MZ\x90\x00 fake exe bytes".to_vec();
        let p = Payload {
            script: script("principal", vec![call("a"), Block::Log { text: "oi {1 + 1}".into() }]),
            called: vec![script("a", vec![Block::Stop])],
            panic: Hotkey::csa(b'Q' as u16),
            idle_resume_s: 3.5,
            ocr_scale: 3,
        };
        let out = append_payload(&exe, &p);
        let (base, got) = split_payload(&out);
        assert_eq!(base, &exe[..]);
        assert_eq!(got, Some(Ok(p.clone())));
        // Reexportar a partir de um exe que já tem payload troca o payload, não empilha.
        let p2 = Payload { ocr_scale: 1, ..p.clone() };
        let again = append_payload(split_payload(&out).0, &p2);
        assert_eq!(split_payload(&again), (&exe[..], Some(Ok(p2))));
        // Pelo arquivo, lendo só o fim.
        let f = std::env::temp_dir().join(format!("ultratask-payload-{}.exe", std::process::id()));
        std::fs::write(&f, &out).unwrap();
        assert_eq!(read_payload_file(&f), Some(Ok(p)));
        std::fs::remove_file(&f).unwrap();
    }

    #[test]
    fn exe_without_payload_opens_the_app() {
        assert_eq!(split_payload(b"MZ normal"), (&b"MZ normal"[..], None));
        assert_eq!(split_payload(b""), (&b""[..], None));
        // O próprio binário de teste é um exe sem nada embutido: main abriria o editor.
        assert_eq!(read_own_payload(), None);
    }

    #[test]
    fn broken_payload_is_an_error_not_the_app() {
        let mut bad = b"MZ".to_vec();
        bad.extend_from_slice(&999u64.to_le_bytes());
        bad.extend_from_slice(MAGIC);
        assert!(matches!(split_payload(&bad).1, Some(Err(e)) if e.contains("tamanho")));
        let mut junk = b"MZ{nao e json".to_vec();
        junk.extend_from_slice(&11u64.to_le_bytes());
        junk.extend_from_slice(MAGIC);
        assert!(matches!(split_payload(&junk).1, Some(Err(e)) if e.contains("inválido")));
        // deny_unknown_fields também vale no payload.
        assert!(serde_json::from_str::<Payload>(r#"{"script":{"name":"x"},"extra":1}"#).is_err());
        assert_eq!(serde_json::from_str::<Payload>(r#"{"script":{"name":"x"}}"#).unwrap().ocr_scale, 2);
    }

    #[test]
    fn real_exe_export_roundtrip() {
        let p = build_payload(&script("p", vec![Block::Stop]), &[], &Settings::default()).unwrap();
        let f = std::env::temp_dir().join(format!("ultratask-export-{}.exe", std::process::id()));
        write_exported(&f, &p).unwrap();
        assert_eq!(read_payload_file(&f), Some(Ok(p)));
        let me = std::fs::metadata(std::env::current_exe().unwrap()).unwrap().len();
        assert!(std::fs::metadata(&f).unwrap().len() > me);
        std::fs::remove_file(&f).unwrap();
        assert!(write_exported(&std::env::current_exe().unwrap(), &Payload::default()).is_err(), "não sobrescreve o exe em uso");
    }

    #[test]
    fn chain_collects_nested_calls_once() {
        let main = script(
            "main",
            vec![
                call("a"),
                Block::If { cond: Default::default(), then: vec![call("b")], else_: vec![Block::Repeat { times: "2".into(), body: vec![call("a")] }] },
                Block::While { cond: Default::default(), body: vec![call("main")] },
            ],
        );
        let all = vec![
            main.clone(),
            script("a", vec![call("c")]),
            script("b", vec![]),
            script("c", vec![call("a")]),
            script("solto", vec![]),
        ];
        let names: Vec<String> = collect_chain(&main, &all).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn export_is_refused_with_a_reason() {
        let settings = Settings { panic: Hotkey::csa(b'Z' as u16), idle_resume_s: 4.0, ocr_scale: 3, ..Settings::default() };
        // Script chamado que não existe, em qualquer nível da cadeia.
        let main = script("main", vec![call("a")]);
        let a = script("a", vec![call("sumiu")]);
        let errs = build_payload(&main, &[main.clone(), a.clone()], &settings).unwrap_err();
        assert!(errs.iter().any(|e| e.starts_with("'a' (chamado)") && e.contains("\"sumiu\" não encontrado")), "{errs:?}");
        // O validador reprova o script principal.
        let bad = script("bad", vec![Block::Wait { ms: "x >".into(), jitter_ms: 0 }]);
        let errs = build_payload(&bad, &[], &settings).unwrap_err();
        assert!(errs.iter().any(|e| e.starts_with("'bad': blocks[0].ms")), "{errs:?}");
        // Sem janela-alvo ou sem blocos.
        let errs = build_payload(&Script { name: "x".into(), ..Default::default() }, &[], &settings).unwrap_err();
        assert_eq!(errs.len(), 2, "{errs:?}");

        // Tudo certo: leva a cadeia e os ajustes, e deixa gatilho e atalho para trás.
        let mut ok = script("main", vec![call("a")]);
        ok.trigger = Trigger::OnStartup { delay_s: 1.0 };
        ok.hotkey = Some(Hotkey::csa(b'K' as u16));
        let a = script("a", vec![Block::Stop]);
        let p = build_payload(&ok, &[ok.clone(), a.clone()], &settings).unwrap();
        assert_eq!(p.called, [a]);
        assert_eq!((p.panic, p.idle_resume_s, p.ocr_scale), (settings.panic, 4.0, 3));
        assert_eq!((p.script.trigger, p.script.hotkey), (Trigger::Manual, None));
        assert_eq!(p.script.blocks, ok.blocks);
        let names: Vec<String> = p.called.iter().map(|s| s.name.clone()).chain([p.script.name.clone()]).collect();
        assert!(validate(&p.script, &names).is_empty());
    }

    #[test]
    fn panic_combo_and_names() {
        let hk = Hotkey::csa(b'X' as u16);
        assert!(combo_down(&hk, |vk| [0x11, 0x10, 0x12, b'X' as u16].contains(&vk)));
        assert!(!combo_down(&hk, |vk| [0x11, 0x10, b'X' as u16].contains(&vk)), "falta o Alt");
        assert!(!combo_down(&Hotkey::default(), |_| true), "atalho vazio nunca dispara");
        let win = Hotkey { win: true, vk: b'X' as u16, ..Default::default() };
        assert!(combo_down(&win, |vk| vk == 0x5C || vk == b'X' as u16));
        assert_eq!(default_exe_name("Meu: script"), "Meu_ script.exe");
        assert_eq!(runner::exit_code(&Ok(())), 0);
        assert_eq!(runner::exit_code(&Err(runner::Halt::Error("x".into()))), 1);
        assert_eq!(runner::exit_code(&Err(runner::Halt::Stopped)), 2);
    }
}
