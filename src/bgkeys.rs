//! Segundo plano: Ctrl, Alt e Shift confiáveis (ticket 14).
//!
//! O app-alvo lê os modificadores com GetKeyState, que não muda com teclas postadas. Enquanto a
//! execução segura um modificador, cada tecla postada vai com o estado do teclado falsificado só
//! para a thread do alvo (técnica do pywinauto): AttachThreadInput, SetKeyboardState, PostMessage,
//! espera curta, estado restaurado e attach desfeito. O teclado real (GetAsyncKeyState) e a janela
//! ativa do usuário não mudam.
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, SetKeyboardState};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Quanto o estado falsificado fica de pé depois que o alvo tirou a tecla da fila.
const HOLD_MS: u64 = 10;
/// Tempo máximo para o alvo responder antes de ser tratado como travado.
const HUNG_MS: u32 = 150;

/// Modificadores que a execução mantém pressionados, lado a lado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub lshift: bool,
    pub rshift: bool,
    pub lctrl: bool,
    pub rctrl: bool,
    pub lalt: bool,
    pub ralt: bool,
}

impl Mods {
    pub fn any(&self) -> bool {
        self.shift() || self.ctrl() || self.alt()
    }
    pub fn shift(&self) -> bool {
        self.lshift || self.rshift
    }
    pub fn ctrl(&self) -> bool {
        self.lctrl || self.rctrl
    }
    pub fn alt(&self) -> bool {
        self.lalt || self.ralt
    }
}

pub fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5)
}

/// Modificadores presos a partir das teclas seguradas, como `(vk, estendida)`. O VK genérico
/// (0x10–0x12) vira o lado esquerdo, ou o direito quando vem com o bit de tecla estendida
/// (Ctrl e Alt da direita são estendidos).
pub fn held_mods(keys: impl IntoIterator<Item = (u16, bool)>) -> Mods {
    let mut m = Mods::default();
    for (vk, ext) in keys {
        match vk {
            0xA0 => m.lshift = true,
            0xA1 => m.rshift = true,
            0x10 => m.lshift = true,
            0xA2 => m.lctrl = true,
            0xA3 => m.rctrl = true,
            0x11 if ext => m.rctrl = true,
            0x11 => m.lctrl = true,
            0xA4 => m.lalt = true,
            0xA5 => m.ralt = true,
            0x12 if ext => m.ralt = true,
            0x12 => m.lalt = true,
            _ => {}
        }
    }
    m
}

/// Estado de teclado a montar para o alvo: liga o bit 0x80 dos modificadores presos (genérico e
/// lado) e desliga o dos que não estão, para a combinação sair exata. Os bits de alternância
/// (0x01, Caps Lock etc.) e as demais teclas ficam como estão.
pub fn forged_state(base: &[u8; 256], m: Mods) -> [u8; 256] {
    let mut s = *base;
    let mut put = |vk: usize, down: bool| s[vk] = if down { s[vk] | 0x80 } else { s[vk] & !0x80 };
    for (generic, left, right, l, r) in [(0x10, 0xA0, 0xA1, m.lshift, m.rshift), (0x11, 0xA2, 0xA3, m.lctrl, m.rctrl), (0x12, 0xA4, 0xA5, m.lalt, m.ralt)] {
        put(left, l);
        put(right, r);
        put(generic, l || r);
    }
    s
}

/// Caminho de uma tecla postada no Segundo plano.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPath {
    /// Como antes deste ticket: só o PostMessage.
    Plain,
    /// Postar com estes modificadores falsificados na thread do alvo.
    Forge(Mods),
    /// O alvo não responde: não postar, porque o attach congelaria a execução.
    Hung,
}

/// Teclas sem modificador preso (e os próprios modificadores) seguem o caminho de hoje. A
/// checagem de travamento custa uma ida e volta ao alvo, então só roda quando vai haver attach.
pub fn plan(vk: u16, held: Mods, hung: impl FnOnce() -> bool) -> KeyPath {
    if is_modifier(vk) || !held.any() {
        KeyPath::Plain
    } else if hung() {
        KeyPath::Hung
    } else {
        KeyPath::Forge(held)
    }
}

/// Nome legível da combinação, para o log ("Ctrl+Shift+S").
pub fn combo_name(m: Mods, vk: u16) -> String {
    let mut s = String::new();
    for (on, name) in [(m.ctrl(), "Ctrl+"), (m.alt(), "Alt+"), (m.shift(), "Shift+")] {
        if on {
            s.push_str(name);
        }
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => s.push(vk as u8 as char),
        0x70..=0x87 => s.push_str(&format!("F{}", vk - 0x6F)),
        0x0D => s.push_str("Enter"),
        0x09 => s.push_str("Tab"),
        0x1B => s.push_str("Esc"),
        0x20 => s.push_str("Espaço"),
        0x2E => s.push_str("Delete"),
        _ => s.push_str(&format!("tecla 0x{vk:02X}")),
    }
    s
}

/// AttachThreadInput exige que as duas threads tenham fila de mensagens; isto cria a nossa.
fn ensure_queue() {
    let mut msg = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
    }
}

/// Janela que não responde em `HUNG_MS` (ou que o Windows já marcou como travada). Como o
/// ControlSend do AutoHotkey, não fazemos attach nela: as duas threads congelariam.
pub fn is_hung(h: HWND) -> bool {
    unsafe {
        if IsHungAppWindow(h).as_bool() {
            return true;
        }
        let mut res = 0usize;
        SendMessageTimeoutW(h, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG | SMTO_BLOCK, HUNG_MS, Some(&mut res)).0 == 0
    }
}

/// Roda `post` com os modificadores `m` falsificados na thread dona de `dest`, e restaura tudo.
/// Erro quando não dá para fazer o attach; quem chama decide se posta mesmo assim.
pub fn post_with_mods(dest: HWND, m: Mods, post: impl FnOnce()) -> Result<(), String> {
    unsafe {
        let me = GetCurrentThreadId();
        let tid = GetWindowThreadProcessId(dest, None);
        if tid == 0 {
            return Err("janela-alvo inválida".into());
        }
        if tid == me {
            return Err("a janela-alvo é da própria thread da execução".into());
        }
        ensure_queue();
        if !AttachThreadInput(me, tid, true).as_bool() {
            return Err("AttachThreadInput falhou (o alvo pode estar elevado)".into());
        }
        // O attach zera o estado das teclas: lemos o estado já compartilhado e o devolvemos no fim.
        let mut saved = [0u8; 256];
        let _ = GetKeyboardState(&mut saved);
        let _ = SetKeyboardState(&forged_state(&saved, m));
        post();
        // Ponto de sincronia: as mensagens enviadas são atendidas no próximo GetMessage do alvo,
        // logo depois de ele tirar a tecla postada da fila.
        let _ = SendMessageTimeoutW(dest, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, HUNG_MS, None);
        std::thread::sleep(std::time::Duration::from_millis(HOLD_MS));
        let _ = SetKeyboardState(&saved);
        let _ = AttachThreadInput(me, tid, false);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_mods_sides() {
        assert_eq!(held_mods([]), Mods::default());
        assert_eq!(held_mods([(0x41, false)]), Mods::default(), "tecla comum não é modificador");
        let m = held_mods([(0x11, false), (0x53, false)]);
        assert!(m.lctrl && !m.rctrl && !m.shift() && !m.alt());
        assert!(held_mods([(0x11, true)]).rctrl, "Ctrl estendido é o da direita");
        assert!(held_mods([(0xA5, false)]).ralt);
        assert!(held_mods([(0x12, true)]).ralt);
        assert!(held_mods([(0x10, false)]).lshift);
        assert!(held_mods([(0xA1, false)]).rshift);
        let all = held_mods([(0xA2, false), (0xA0, false), (0xA4, false)]);
        assert!(all.ctrl() && all.shift() && all.alt());
    }

    #[test]
    fn forged_state_sets_and_clears() {
        let mut base = [0u8; 256];
        base[0x14] = 0x01; // Caps Lock ligado: alternância preservada
        base[0x10] = 0x80; // Shift sobrando no estado lido: sai, a combinação é só Ctrl
        base[0xA0] = 0x81;
        base[0x41] = 0x80; // outra tecla: intocada
        let s = forged_state(&base, Mods { lctrl: true, ..Default::default() });
        assert_eq!(s[0x11], 0x80);
        assert_eq!(s[0xA2], 0x80);
        assert_eq!(s[0xA3], 0x00);
        assert_eq!(s[0x10], 0x00);
        assert_eq!(s[0xA0], 0x01);
        assert_eq!(s[0x14], 0x01);
        assert_eq!(s[0x41], 0x80);
        let s = forged_state(&[0u8; 256], Mods { ralt: true, rshift: true, ..Default::default() });
        assert_eq!((s[0x12], s[0xA4], s[0xA5]), (0x80, 0, 0x80));
        assert_eq!((s[0x10], s[0xA0], s[0xA1]), (0x80, 0, 0x80));
        assert_eq!(s[0x11], 0);
    }

    #[test]
    fn plan_paths() {
        let ctrl = Mods { lctrl: true, ..Default::default() };
        let never = || -> bool { panic!("checagem de travamento não deveria rodar") };
        // Sem modificador preso: caminho de hoje, sem custo extra.
        assert_eq!(plan(0x53, Mods::default(), never), KeyPath::Plain);
        // O próprio modificador também segue o caminho de hoje.
        assert_eq!(plan(0x11, ctrl, never), KeyPath::Plain);
        assert_eq!(plan(0xA4, ctrl, never), KeyPath::Plain);
        assert_eq!(plan(0x53, ctrl, || false), KeyPath::Forge(ctrl));
        assert_eq!(plan(0x53, ctrl, || true), KeyPath::Hung);
    }

    #[test]
    fn combo_names() {
        assert_eq!(combo_name(Mods { lctrl: true, ..Default::default() }, 0x53), "Ctrl+S");
        assert_eq!(combo_name(Mods { rctrl: true, lshift: true, ..Default::default() }, 0x31), "Ctrl+Shift+1");
        assert_eq!(combo_name(Mods { lalt: true, ..Default::default() }, 0x73), "Alt+F4");
        assert_eq!(combo_name(Mods::default(), 0xBA), "tecla 0xBA");
    }

    /// Janela própria com wndproc que anota GetKeyState(VK_CONTROL) ao receber o 'S' postado.
    /// Com o Ctrl preso pela execução, o alvo tem que ver o Ctrl; o sistema (GetAsyncKeyState), não.
    #[test]
    #[ignore = "abre uma janela de verdade e usa AttachThreadInput; rode no Windows com cargo test -- --ignored"]
    fn e2e_ctrl_s_forged() {
        use crate::model::{Ev, Mode};
        use crate::win::Injector;
        use std::sync::atomic::{AtomicI32, Ordering};
        use windows::Win32::Foundation::{HINSTANCE, LRESULT};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
        use windows::core::w;
        static SEEN: AtomicI32 = AtomicI32::new(-1);
        unsafe extern "system" fn proc_(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe {
                if m == WM_KEYDOWN && wp.0 == 0x53 {
                    SEEN.store((GetKeyState(0x11) as u16 & 0x8000 != 0) as i32, Ordering::SeqCst);
                }
                DefWindowProcW(h, m, wp, lp)
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let inst = HINSTANCE(GetModuleHandleW(None).unwrap().0);
            let wc = WNDCLASSW { lpfnWndProc: Some(proc_), hInstance: inst, lpszClassName: w!("ultratask-e2e-mods"), ..Default::default() };
            RegisterClassW(&wc);
            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("ultratask-e2e-mods"), w!("ultratask-e2e-mods"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 140, 140, 300, 200, None, None, Some(inst), None).unwrap();
            tx.send(top.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let top = rx.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let fg = unsafe { GetForegroundWindow() };
        for i in 0..20 {
            SEEN.store(-1, Ordering::SeqCst);
            let mut inj = Injector::new(top, Mode::Background, false);
            inj.send(&Ev::Key(0x11, 0, false, true));
            inj.send(&Ev::Key(0x53, 0, false, true));
            assert!(unsafe { GetAsyncKeyState(0x11) } as u16 & 0x8000 == 0, "Ctrl real apareceu pressionado");
            inj.send(&Ev::Key(0x53, 0, false, false));
            inj.send(&Ev::Key(0x11, 0, false, false));
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert_eq!(SEEN.load(Ordering::SeqCst), 1, "tentativa {i}: o alvo não viu o Ctrl");
            assert!(inj.notes.is_empty(), "{:?}", inj.notes);
        }
        assert_eq!(unsafe { GetForegroundWindow() }, fg, "a janela ativa mudou");
    }

    /// Alvo cuja thread para de bombear mensagens: a combinação falha com aviso, sem congelar.
    #[test]
    #[ignore = "abre uma janela de verdade e a deixa travada por 3 s; rode no Windows com cargo test -- --ignored"]
    fn e2e_hung_target_does_not_freeze() {
        use crate::model::{Ev, Mode};
        use crate::win::Injector;
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::core::w;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let inst = Some(HINSTANCE(GetModuleHandleW(None).unwrap().0));
            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("ultratask-e2e-hung"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 160, 160, 300, 200, None, None, inst, None).unwrap();
            tx.send(top.0 as isize).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3)); // travada: não bombeia
            let _ = DestroyWindow(top);
        });
        let top = rx.recv().unwrap();
        let t0 = std::time::Instant::now();
        let mut inj = Injector::new(top, Mode::Background, false);
        inj.send(&Ev::Key(0x11, 0, false, true));
        inj.send(&Ev::Key(0x53, 0, false, true));
        inj.send(&Ev::Key(0x53, 0, false, false));
        inj.send(&Ev::Key(0x11, 0, false, false));
        assert!(t0.elapsed() < std::time::Duration::from_secs(1), "a execução congelou: {:?}", t0.elapsed());
        assert!(inj.notes.iter().any(|n| n.contains("não responde") && n.contains("Ctrl+S")), "{:?}", inj.notes);
    }
}
