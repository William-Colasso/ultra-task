//! Thin Win32 layer: window lookup, input injection (background + focus), capture and OCR.
use crate::model::{Btn, Ev, Mode, Region, TargetRule};
use std::mem::size_of;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR, w};

/// HWND as a plain integer so it can cross threads.
pub type Hwnd = isize;

pub fn hw(h: Hwnd) -> HWND {
    HWND(h as *mut _)
}

fn raw(h: HWND) -> Hwnd {
    h.0 as isize
}

#[derive(Clone, Debug, PartialEq)]
pub struct WinInfo {
    pub hwnd: Hwnd,
    pub title: String,
    pub exe: String,
    pub pid: u32,
}

impl WinInfo {
    pub fn label(&self) -> String {
        format!("{} — {}", self.title, self.exe)
    }
}

pub fn title_of(h: Hwnd) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hw(h), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn exe_of_pid(pid: u32) -> String {
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(p);
        if !ok {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit('\\').next().unwrap_or_default().to_string()
    }
}

pub fn window_info(h: Hwnd) -> WinInfo {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hw(h), Some(&mut pid)) };
    WinInfo { hwnd: h, title: title_of(h), exe: exe_of_pid(pid), pid }
}

/// Visible, titled, non-tool top-level windows.
pub fn list_windows() -> Vec<WinInfo> {
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        unsafe {
            let out = &mut *(lp.0 as *mut Vec<Hwnd>);
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            if IsWindowVisible(h).as_bool() && ex & WS_EX_TOOLWINDOW.0 == 0 && GetWindowTextLengthW(h) > 0 {
                out.push(raw(h));
            }
        }
        BOOL(1)
    }
    let mut hs: Vec<Hwnd> = vec![];
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut hs as *mut _ as isize));
    }
    let me = std::process::id();
    hs.into_iter().map(window_info).filter(|w| w.pid != me).collect()
}

pub fn find_matches(rule: &TargetRule) -> Vec<WinInfo> {
    list_windows().into_iter().filter(|w| rule.matches(&w.exe, &w.title)).collect()
}

pub fn is_alive(h: Hwnd) -> bool {
    unsafe { IsWindow(Some(hw(h))).as_bool() }
}

pub fn is_foreground(h: Hwnd) -> bool {
    unsafe { raw(GetForegroundWindow()) == h }
}

pub fn root_at(x: i32, y: i32) -> Hwnd {
    unsafe { raw(GetAncestor(WindowFromPoint(POINT { x, y }), GA_ROOT)) }
}

pub fn client_size(h: Hwnd) -> Option<(i32, i32)> {
    let mut r = RECT::default();
    unsafe { GetClientRect(hw(h), &mut r).ok()? };
    Some((r.right - r.left, r.bottom - r.top))
}

pub fn screen_to_client(h: Hwnd, x: i32, y: i32) -> (i32, i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ScreenToClient(hw(h), &mut p);
    }
    (p.x, p.y)
}

/// Posição do cursor real, em coordenadas de cliente de `h`.
pub fn cursor_client(h: Hwnd) -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    screen_to_client(h, p.x, p.y)
}

fn client_to_screen(h: Hwnd, x: i32, y: i32) -> (i32, i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ClientToScreen(hw(h), &mut p);
    }
    (p.x, p.y)
}

/// Makes the client area exactly `w`×`h` so recorded coordinates line up.
pub fn resize_client(hwnd: Hwnd, w: i32, h: i32) -> Result<(), String> {
    let (cw, ch) = client_size(hwnd).ok_or("janela inválida")?;
    if (cw, ch) == (w, h) {
        return Ok(());
    }
    unsafe {
        if IsZoomed(hw(hwnd)).as_bool() {
            return Err(format!("janela maximizada: área {cw}×{ch}, gravado {w}×{h}"));
        }
        let mut r = RECT::default();
        GetWindowRect(hw(hwnd), &mut r).map_err(|e| e.to_string())?;
        let (nw, nh) = (r.right - r.left + w - cw, r.bottom - r.top + h - ch);
        SetWindowPos(hw(hwnd), None, 0, 0, nw, nh, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE).map_err(|e| e.to_string())?;
    }
    match client_size(hwnd) {
        Some(s) if s == (w, h) => Ok(()),
        Some((a, b)) => Err(format!("não consegui redimensionar: área {a}×{b}, gravado {w}×{h}")),
        None => Err("janela inválida".into()),
    }
}

/// True when the process runs elevated, or when we can't even query it (likely elevated).
pub fn is_elevated_pid(pid: u32) -> bool {
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return true };
        let mut tok = HANDLE::default();
        let elevated = if windows::Win32::System::Threading::OpenProcessToken(p, TOKEN_QUERY, &mut tok).is_ok() {
            let mut e = TOKEN_ELEVATION::default();
            let mut len = 0;
            let ok = GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as *mut _), size_of::<TOKEN_ELEVATION>() as u32, &mut len).is_ok();
            let _ = CloseHandle(tok);
            !ok || e.TokenIsElevated != 0
        } else {
            true
        };
        let _ = CloseHandle(p);
        elevated
    }
}

pub fn self_elevated() -> bool {
    is_elevated_pid(std::process::id())
}

pub fn relaunch_as_admin() {
    let Ok(exe) = std::env::current_exe() else { return };
    let r = unsafe { ShellExecuteW(None, w!("runas"), &HSTRING::from(exe.as_os_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if r.0 as isize > 32 {
        std::process::exit(0);
    }
}

/// Writes (Some) or deletes (None) a REG_SZ value under HKEY_CURRENT_USER.
fn set_reg_string(subkey: PCWSTR, name: PCWSTR, value: Option<&str>) -> Result<(), String> {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
    let r = unsafe {
        match value {
            Some(v) => {
                let wide: Vec<u16> = v.encode_utf16().chain([0]).collect();
                RegSetKeyValueW(HKEY_CURRENT_USER, subkey, name, REG_SZ.0, Some(wide.as_ptr() as *const _), (wide.len() * 2) as u32)
            }
            None => match RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey, name) {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                r => r,
            },
        }
    };
    if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("registro do Windows: erro {}", r.0)) }
}

/// "Iniciar com o Windows": HKCU Run entry that starts us hidden in the tray.
pub fn set_autostart(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cmd = format!("\"{}\" --tray", exe.display());
    set_reg_string(w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"), w!("UltraTask"), on.then_some(cmd.as_str()))
}

/// First instance: returns the named "show" event to wait on. A second instance signals that
/// event (so the running one pops up) and gets None, meaning it should exit.
pub fn single_instance() -> Option<isize> {
    unsafe {
        if let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, w!("UltraTask.Show")) {
            let _ = SetEvent(h);
            let _ = CloseHandle(h);
            return None;
        }
        CreateEventW(None, false, false, w!("UltraTask.Show")).ok().map(|h| h.0 as isize)
    }
}

/// True if another instance asked us to show ourselves within `ms`.
pub fn wait_show(ev: isize, ms: u32) -> bool {
    unsafe { WaitForSingleObject(HANDLE(ev as *mut _), ms) == windows::Win32::Foundation::WAIT_OBJECT_0 }
}

fn send_inputs(inputs: &[INPUT]) {
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

/// Restores and foregrounds `h`. The zero-length injected move makes us "the last input
/// source", which Windows requires before it lets SetForegroundWindow through.
pub fn bring_to_front(h: Hwnd) {
    unsafe {
        if IsIconic(hw(h)).as_bool() {
            let _ = ShowWindow(hw(h), SW_RESTORE);
        }
        send_inputs(&[mouse_input(0, 0, 0, MOUSEEVENTF_MOVE)]);
        let _ = SetForegroundWindow(hw(h));
    }
}

/// Keys whose scancode needs the extended (E0) prefix.
fn is_extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C | 0x5D | 0x6F | 0x90 | 0xA3 | 0xA5)
}

pub fn lparam_xy(x: i32, y: i32) -> isize {
    (((y as u16 as u32) << 16) | x as u16 as u32) as isize
}

pub fn key_lparam(scan: u16, ext: bool, up: bool, alt: bool) -> isize {
    let mut l = 1u32 | ((scan as u32 & 0xFF) << 16);
    if ext {
        l |= 1 << 24;
    }
    if alt {
        l |= 1 << 29;
    }
    if up {
        l |= (1 << 30) | (1 << 31);
    }
    l as isize
}

fn btn_bits(b: Btn) -> (u32, u32, usize, MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
    // (down msg, up msg, MK_* bit, SendInput down, SendInput up)
    match b {
        Btn::Left => (WM_LBUTTONDOWN, WM_LBUTTONUP, 0x01, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        Btn::Right => (WM_RBUTTONDOWN, WM_RBUTTONUP, 0x02, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        Btn::Middle => (WM_MBUTTONDOWN, WM_MBUTTONUP, 0x10, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    }
}

/// Delivers recorded/synthesized input to one target window, tracking what is held so a
/// pause or stop never leaves a key stuck down.
pub struct Injector {
    pub top: Hwnd,
    pub mode: Mode,
    pub simulate_focus: bool,
    buttons: Vec<Btn>,
    keys: Vec<(u16, u16, bool)>,
    last_child: Hwnd,
    last_pos: (i32, i32),
    /// Avisos para o log da execução (ex.: combinação não enviada a um alvo travado).
    pub notes: Vec<String>,
}

impl Injector {
    pub fn new(top: Hwnd, mode: Mode, simulate_focus: bool) -> Self {
        Injector { top, mode, simulate_focus, buttons: vec![], keys: vec![], last_child: 0, last_pos: (0, 0), notes: vec![] }
    }

    pub fn prepare(&mut self) {
        match self.mode {
            Mode::Focus => bring_to_front(self.top),
            Mode::Background if self.simulate_focus => unsafe {
                let t = Some(hw(self.top));
                let _ = PostMessageW(t, WM_ACTIVATEAPP, WPARAM(1), LPARAM(0));
                let _ = PostMessageW(t, WM_ACTIVATE, WPARAM(1), LPARAM(0)); // WA_ACTIVE
                let _ = PostMessageW(Some(self.focus_target()), WM_SETFOCUS, WPARAM(0), LPARAM(0));
            },
            Mode::Background => {}
        }
    }

    fn mask(&self) -> usize {
        self.buttons.iter().map(|&b| btn_bits(b).2).fold(0, |a, b| a | b)
    }

    /// Deepest enabled, visible child under a client point of the top window.
    fn child_at(&self, x: i32, y: i32) -> (HWND, i32, i32) {
        let mut h = hw(self.top);
        let mut pt = POINT { x, y };
        unsafe {
            loop {
                let c = ChildWindowFromPointEx(h, pt, CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT);
                if c.is_invalid() || c == h {
                    break;
                }
                MapWindowPoints(Some(h), Some(c), std::slice::from_mut(&mut pt));
                h = c;
            }
        }
        (h, pt.x, pt.y)
    }

    /// Keyboard destination. The control we last clicked wins: GetGUIThreadInfo would report
    /// the focus from *before* the target processed our still-queued click.
    fn focus_target(&self) -> HWND {
        if self.last_child != 0 && is_alive(self.last_child) {
            return hw(self.last_child);
        }
        unsafe {
            let tid = GetWindowThreadProcessId(hw(self.top), None);
            let mut gi = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
            if GetGUIThreadInfo(tid, &mut gi).is_ok() && !gi.hwndFocus.is_invalid() {
                return gi.hwndFocus;
            }
        }
        hw(self.top)
    }

    fn post_mouse(&mut self, msg: u32, x: i32, y: i32, extra: usize) -> HWND {
        let (child, cx, cy) = self.child_at(x, y);
        unsafe {
            let _ = PostMessageW(Some(child), msg, WPARAM(self.mask() | extra), LPARAM(lparam_xy(cx, cy)));
        }
        child
    }

    fn abs_move(&self, x: i32, y: i32, flags: MOUSE_EVENT_FLAGS, data: u32) -> INPUT {
        let (sx, sy) = client_to_screen(self.top, x, y);
        let (vx, vy, vw, vh) = unsafe {
            (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN), GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN))
        };
        let dx = ((sx - vx) as i64 * 65535 / (vw - 1).max(1) as i64) as i32;
        let dy = ((sy - vy) as i64 * 65535 / (vh - 1).max(1) as i64) as i32;
        mouse_input(dx, dy, data, flags | MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)
    }

    pub fn send(&mut self, ev: &Ev) {
        if let Ev::Move(x, y) | Ev::Button(_, _, x, y) | Ev::Wheel(_, x, y) = *ev {
            self.last_pos = (x, y);
        }
        match (*ev, self.mode) {
            (Ev::Move(x, y), Mode::Background) => {
                self.post_mouse(WM_MOUSEMOVE, x, y, 0);
            }
            (Ev::Move(x, y), Mode::Focus) => send_inputs(&[self.abs_move(x, y, MOUSE_EVENT_FLAGS(0), 0)]),
            (Ev::Button(b, down, x, y), mode) => {
                let (dm, um, bit, fd, fu) = btn_bits(b);
                if down {
                    self.buttons.retain(|&o| o != b);
                    self.buttons.push(b);
                } else {
                    self.buttons.retain(|&o| o != b);
                }
                match mode {
                    Mode::Background => {
                        self.post_mouse(WM_MOUSEMOVE, x, y, 0);
                        // MK_* for the changed button is set on down and cleared on up by mask().
                        let child = self.post_mouse(if down { dm } else { um }, x, y, if down { bit } else { 0 });
                        if down {
                            self.last_child = raw(child);
                        }
                    }
                    Mode::Focus => send_inputs(&[self.abs_move(x, y, if down { fd } else { fu }, 0)]),
                }
            }
            (Ev::Wheel(delta, x, y), Mode::Background) => {
                let (sx, sy) = client_to_screen(self.top, x, y);
                let (child, _, _) = self.child_at(x, y);
                unsafe {
                    let _ = PostMessageW(Some(child), WM_MOUSEWHEEL, WPARAM(((delta as u16 as usize) << 16) | self.mask()), LPARAM(lparam_xy(sx, sy)));
                }
            }
            (Ev::Wheel(delta, x, y), Mode::Focus) => send_inputs(&[self.abs_move(x, y, MOUSEEVENTF_WHEEL, delta as u32)]),
            (Ev::Key(vk, scan, ext, down), mode) => {
                let scan = if scan != 0 { scan } else { unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16 } };
                let ext = ext || is_extended(vk);
                if down {
                    if !self.keys.iter().any(|k| k.0 == vk) {
                        self.keys.push((vk, scan, ext));
                    }
                } else {
                    self.keys.retain(|k| k.0 != vk);
                }
                match mode {
                    Mode::Background => {
                        let alt = self.keys.iter().any(|k| matches!(k.0, 0x12 | 0xA4 | 0xA5)) || matches!(vk, 0x12 | 0xA4 | 0xA5);
                        let msg = match (down, alt) {
                            (true, false) => WM_KEYDOWN,
                            (false, false) => WM_KEYUP,
                            (true, true) => WM_SYSKEYDOWN,
                            (false, true) => WM_SYSKEYUP,
                        };
                        let dest = self.focus_target();
                        let post = || unsafe {
                            let _ = PostMessageW(Some(dest), msg, WPARAM(vk as usize), LPARAM(key_lparam(scan, ext, !down, alt)));
                        };
                        // Ticket 14: com Ctrl/Alt/Shift presos, o alvo precisa vê-los em GetKeyState.
                        let held = crate::bgkeys::held_mods(self.keys.iter().map(|k| (k.0, k.2)));
                        match crate::bgkeys::plan(vk, held, || crate::bgkeys::is_hung(dest)) {
                            crate::bgkeys::KeyPath::Plain => post(),
                            crate::bgkeys::KeyPath::Forge(m) => {
                                if let Err(e) = crate::bgkeys::post_with_mods(dest, m, post) {
                                    self.notes.push(format!("aviso: {} enviado sem modificadores: {e}", crate::bgkeys::combo_name(m, vk)));
                                    post();
                                }
                            }
                            crate::bgkeys::KeyPath::Hung => {
                                self.notes.push(format!("aviso: a janela-alvo não responde; {} não foi enviado", crate::bgkeys::combo_name(held, vk)));
                            }
                        }
                    }
                    Mode::Focus => {
                        // Scancodes: games reading DirectInput ignore virtual-key-only input.
                        let mut f = KEYEVENTF_SCANCODE;
                        if ext {
                            f |= KEYEVENTF_EXTENDEDKEY;
                        }
                        if !down {
                            f |= KEYEVENTF_KEYUP;
                        }
                        send_inputs(&[key_input(0, scan, f)]);
                    }
                }
            }
        }
    }

    pub fn type_text(&mut self, s: &str) {
        match self.mode {
            // ponytail: WM_CHAR goes straight to the queue, while a posted WM_KEYDOWN only becomes a
            // char when the target runs TranslateMessage — the runner's 20 ms press delay keeps them in order.
            Mode::Background => {
                let t = Some(self.focus_target());
                for u in s.encode_utf16() {
                    unsafe {
                        let _ = PostMessageW(t, WM_CHAR, WPARAM(u as usize), LPARAM(1));
                    }
                }
            }
            Mode::Focus => {
                let inputs: Vec<INPUT> = s
                    .encode_utf16()
                    .flat_map(|u| [key_input(0, u, KEYEVENTF_UNICODE), key_input(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)])
                    .collect();
                send_inputs(&inputs);
            }
        }
    }

    /// Lifts every button/key this injector pressed.
    pub fn release_all(&mut self) {
        for b in std::mem::take(&mut self.buttons).into_iter().rev() {
            let (x, y) = self.last_pos;
            match self.mode {
                Mode::Background => {
                    self.post_mouse(btn_bits(b).1, x, y, 0);
                }
                Mode::Focus => send_inputs(&[mouse_input(0, 0, 0, btn_bits(b).4)]),
            }
        }
        for (vk, scan, ext) in std::mem::take(&mut self.keys).into_iter().rev() {
            self.send(&Ev::Key(vk, scan, ext, false));
        }
    }
}

// ---------- capture ----------

pub struct Image {
    pub w: i32,
    pub h: i32,
    pub bgra: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        let i = ((y * self.w + x) * 4) as usize;
        Some([self.bgra[i + 2], self.bgra[i + 1], self.bgra[i]])
    }

    pub fn crop(&self, r: Region) -> Image {
        let (x0, y0) = (r.x.clamp(0, self.w), r.y.clamp(0, self.h));
        let (x1, y1) = ((r.x + r.w).clamp(x0, self.w), (r.y + r.h).clamp(y0, self.h));
        let mut bgra = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
        for y in y0..y1 {
            let s = ((y * self.w + x0) * 4) as usize;
            bgra.extend_from_slice(&self.bgra[s..s + ((x1 - x0) * 4) as usize]);
        }
        Image { w: x1 - x0, h: y1 - y0, bgra }
    }

    /// Nearest-neighbour upscale; small UI text OCRs much better at 2×.
    pub fn scale(&self, k: u32) -> Image {
        let k = k.max(1) as i32;
        let (w, h) = (self.w * k, self.h * k);
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let i = (((y / k) * self.w + x / k) * 4) as usize;
                bgra.extend_from_slice(&self.bgra[i..i + 4]);
            }
        }
        Image { w, h, bgra }
    }
}

/// Background: PrintWindow (works behind other windows). Focus: screen copy (sees DirectX content).
pub fn capture_client(hwnd: Hwnd, mode: Mode) -> Result<Image, String> {
    unsafe {
        if IsIconic(hw(hwnd)).as_bool() {
            return Err("janela-alvo minimizada: não dá para capturar".into());
        }
        let (w, h) = client_size(hwnd).ok_or("janela inválida")?;
        if w <= 0 || h <= 0 {
            return Err("janela sem área visível".into());
        }
        let src = if mode == Mode::Focus { GetDC(None) } else { GetDC(Some(hw(hwnd))) };
        let mem = CreateCompatibleDC(Some(src));
        let bmp = CreateCompatibleBitmap(src, w, h);
        let old = SelectObject(mem, bmp.into());
        let ok = if mode == Mode::Focus {
            let (sx, sy) = client_to_screen(hwnd, 0, 0);
            BitBlt(mem, 0, 0, w, h, Some(src), sx, sy, SRCCOPY).is_ok()
        } else {
            // PW_CLIENTONLY | PW_RENDERFULLCONTENT
            PrintWindow(hw(hwnd), mem, PRINT_WINDOW_FLAGS(3)).as_bool()
        };
        SelectObject(mem, old);
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(bgra.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(if mode == Mode::Focus { None } else { Some(hw(hwnd)) }, src);
        if !ok || lines == 0 {
            return Err("falha ao capturar a janela".into());
        }
        Ok(Image { w, h, bgra })
    }
}

/// Windows' built-in OCR (languages installed in the user profile).
pub fn ocr(img: &Image) -> Result<String, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    if img.w == 0 || img.h == 0 {
        return Ok(String::new());
    }
    let e = |e: windows::core::Error| format!("OCR: {}", e.message());
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    // Sem idioma com OCR o Windows devolve um motor nulo (Err aqui): explique como instalar.
    let profile = OcrEngine::TryCreateFromUserProfileLanguages();
    let langs = OcrEngine::AvailableRecognizerLanguages().ok();
    let n = langs.as_ref().and_then(|l| l.Size().ok()).unwrap_or(0) as usize;
    let engine = match (crate::compat::choose_ocr(profile.is_ok(), n), profile) {
        (crate::compat::OcrChoice::UserProfile, Ok(engine)) => engine,
        (crate::compat::OcrChoice::FirstInstalled, _) => langs.and_then(|l| l.GetAt(0).ok()).and_then(|l| OcrEngine::TryCreateFromLanguage(&l).ok()).ok_or_else(|| crate::compat::ocr_missing_message(""))?,
        (_, p) => return Err(crate::compat::ocr_missing_message(&p.err().map(|e| e.message()).unwrap_or_default())),
    };
    let buf = CryptographicBuffer::CreateFromByteArray(&img.bgra).map_err(e)?;
    let bmp = SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Bgra8, img.w, img.h).map_err(e)?;
    let res = engine.RecognizeAsync(&bmp).map_err(e)?.join().map_err(e)?;
    Ok(res.Text().map_err(e)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lparams() {
        assert_eq!(lparam_xy(10, 20), (20 << 16) | 10);
        assert_eq!(lparam_xy(-1, 0), 0xFFFF);
        // 'A' down: repeat 1, scan 0x1E.
        assert_eq!(key_lparam(0x1E, false, false, false), 0x001E_0001);
        // up sets bits 30/31; extended sets bit 24.
        assert_eq!(key_lparam(0x48, true, true, false) as u32, 0xC148_0001);
    }

    /// Real Win32: a window of our own with an EDIT control receives a background click +
    /// typing through PostMessage, then PrintWindow + Windows OCR read it back.
    #[test]
    #[ignore = "abre uma janela de verdade; rode com cargo test -- --ignored"]
    fn e2e_background_edit() {
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let inst = Some(HINSTANCE(GetModuleHandleW(None).unwrap().0));
            let style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("ultratask-e2e"), style, 120, 120, 520, 220, None, None, inst, None).unwrap();
            let estyle = WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_MULTILINE as u32);
            let edit = CreateWindowExW(WINDOW_EX_STYLE(0), w!("EDIT"), w!(""), estyle, 10, 10, 480, 140, Some(top), None, inst, None).unwrap();
            tx.send((raw(top), raw(edit))).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let (top, edit) = rx.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut inj = Injector::new(top, Mode::Background, false);
        inj.send(&Ev::Button(Btn::Left, true, 60, 40));
        inj.send(&Ev::Button(Btn::Left, false, 60, 40));
        assert_eq!(inj.last_child, edit, "child_at should resolve the EDIT under the click");
        inj.type_text("ULTRATASK 42");
        inj.send(&Ev::Key(0x0D, 0, false, true)); // Enter via WM_KEYDOWN → TranslateMessage → newline
        std::thread::sleep(std::time::Duration::from_millis(30)); // the runner's press delay
        inj.send(&Ev::Key(0x0D, 0, false, false));
        inj.type_text("ok");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(title_of(edit), "ULTRATASK 42\r\nok");

        let img = capture_client(top, Mode::Background).unwrap();
        let txt = ocr(&img.scale(2)).unwrap();
        assert!(txt.to_uppercase().contains("ULTRATASK"), "OCR leu: {txt:?}");
    }

    /// Same code path as set_autostart, on a throwaway key that is removed afterwards.
    #[test]
    #[ignore = "escreve numa chave temporária do HKCU; rode com cargo test -- --ignored"]
    fn registry_roundtrip() {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegDeleteKeyW, RegGetValueW};
        let (key, name) = (w!("Software\\UltraTaskTest"), w!("v"));
        let read = || unsafe {
            let mut buf = [0u16; 64];
            let mut len = (buf.len() * 2) as u32;
            let r = RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len));
            r.is_ok().then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
        };
        set_reg_string(key, name, Some("\"C:\\x y\\u.exe\" --tray")).unwrap();
        assert_eq!(read().as_deref(), Some("\"C:\\x y\\u.exe\" --tray"));
        set_reg_string(key, name, None).unwrap();
        assert_eq!(read(), None);
        set_reg_string(key, name, None).unwrap(); // deleting twice is fine
        unsafe {
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, key);
        }
    }

    #[test]
    fn image_ops() {
        let img = Image { w: 2, h: 1, bgra: vec![1, 2, 3, 255, 4, 5, 6, 255] };
        assert_eq!(img.pixel(1, 0), Some([6, 5, 4]));
        assert_eq!(img.pixel(2, 0), None);
        let s = img.scale(2);
        assert_eq!((s.w, s.h), (4, 2));
        assert_eq!(s.pixel(3, 1), Some([6, 5, 4]));
        assert_eq!(img.crop(Region { x: 1, y: 0, w: 5, h: 5 }).bgra, vec![4, 5, 6, 255]);
    }
}
