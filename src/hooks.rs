//! One persistent thread owning a Win32 message loop: global hotkeys (always) and
//! low-level mouse/keyboard hooks (only while recording, picking or a Focus run is active,
//! because an LL mouse hook adds latency to every mouse event system-wide).
use crate::model::{Btn, Ev, Event, Hotkey};
use crate::uia_rec::{self, Click, ClickCollector, RecElem};
use crate::win::{self, Hwnd};
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const RECORD_HOTKEY: i32 = 1;
pub const PLAY_HOTKEY: i32 = 2;
pub const PANIC_HOTKEY: i32 = 3;
/// Per-script hotkeys use `SCRIPT_HOTKEY + index`.
pub const SCRIPT_HOTKEY: i32 = 100;

pub enum Cmd {
    Hotkeys(Vec<(i32, Hotkey)>),
    Record(Option<Hwnd>), // Some = start on that window, None = stop
    Pick,
}

pub enum Out {
    Hotkey(i32),
    Recorded(Vec<Event>, Vec<RecElem>),
    Picked { x: i32, y: i32, root: Hwnd },
    Error(String),
}

/// Number of Focus runs currently wanting physical-input detection.
static FOCUS_RUNS: AtomicU32 = AtomicU32::new(0);
/// GetTickCount64() of the last physical (non-injected) mouse event or key-down.
pub static LAST_PHYSICAL: AtomicU64 = AtomicU64::new(0);
static HOOK_TID: AtomicU32 = AtomicU32::new(0);
/// Recordings whose clicks' UIA elements were resolved off this thread, waiting to be emitted.
static FINISHED: Mutex<Vec<(Vec<Event>, Vec<RecElem>)>> = Mutex::new(Vec::new());

pub fn tick() -> u64 {
    unsafe { GetTickCount64() }
}

fn wake_thread() {
    unsafe {
        let _ = PostThreadMessageW(HOOK_TID.load(Ordering::Relaxed), WM_APP, WPARAM(0), LPARAM(0));
    }
}

/// Focus runs call this on start (true) and end (false).
pub fn watch_physical(on: bool) {
    if on {
        FOCUS_RUNS.fetch_add(1, Ordering::SeqCst);
    } else {
        FOCUS_RUNS.fetch_sub(1, Ordering::SeqCst);
    }
    wake_thread();
}

struct Recorder {
    target: Hwnd,
    events: Vec<Event>,
    last_move: (i32, i32),
    /// Resolves the UIA element of each button-down on its own thread.
    clicks: ClickCollector,
    downs: u32,
}

#[derive(Default)]
struct State {
    out: Option<Sender<Out>>,
    wake_ui: Option<Box<dyn Fn()>>,
    rec: Option<Recorder>,
    picking: bool,
    swallow_up: bool,
    hooks: Option<(HHOOK, HHOOK)>,
}

thread_local! {
    // LL hook procs run on the thread that installed them, so thread-local state is enough.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn emit(o: Out) {
    STATE.with(|s| {
        let s = s.borrow();
        if let Some(tx) = &s.out {
            let _ = tx.send(o);
        }
        if let Some(w) = &s.wake_ui {
            w();
        }
    });
}

pub struct Hooks {
    tx: Sender<Cmd>,
}

impl Hooks {
    pub fn start(wake_ui: impl Fn() + Send + 'static) -> (Hooks, Receiver<Out>) {
        let (tx, rx) = channel::<Cmd>();
        let (otx, orx) = channel::<Out>();
        let (ready_tx, ready_rx) = channel();
        std::thread::spawn(move || {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.out = Some(otx);
                s.wake_ui = Some(Box::new(wake_ui));
            });
            unsafe {
                let mut msg = MSG::default();
                // Force creation of this thread's message queue before anyone posts to it.
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                HOOK_TID.store(GetCurrentThreadId(), Ordering::SeqCst);
                let _ = ready_tx.send(());
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    match msg.message {
                        // Stop recording right here so the chord's key-ups never get recorded.
                        WM_HOTKEY if msg.wParam.0 as i32 == RECORD_HOTKEY && STATE.with(|s| s.borrow().rec.is_some()) => {
                            handle(Cmd::Record(None));
                            ensure_hooks();
                        }
                        WM_HOTKEY => emit(Out::Hotkey(msg.wParam.0 as i32)),
                        WM_APP => {
                            while let Ok(c) = rx.try_recv() {
                                handle(c);
                            }
                            let done = std::mem::take(&mut *FINISHED.lock().unwrap_or_else(|e| e.into_inner()));
                            for (ev, els) in done {
                                emit(Out::Recorded(ev, els));
                            }
                            ensure_hooks();
                        }
                        _ => {}
                    }
                }
            }
        });
        let _ = ready_rx.recv();
        (Hooks { tx }, orx)
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
        wake_thread();
    }
}

fn handle(c: Cmd) {
    match c {
        Cmd::Hotkeys(list) => unsafe {
            for id in 1..1000 {
                let _ = UnregisterHotKey(None, id);
            }
            for (id, hk) in list {
                if hk.vk == 0 {
                    continue;
                }
                let mut m = MOD_NOREPEAT;
                for (on, f) in [(hk.ctrl, MOD_CONTROL), (hk.shift, MOD_SHIFT), (hk.alt, MOD_ALT), (hk.win, MOD_WIN)] {
                    if on {
                        m |= f;
                    }
                }
                if RegisterHotKey(None, id, m, hk.vk as u32).is_err() {
                    emit(Out::Error(format!("Atalho {} já está em uso por outro programa", hk.label())));
                }
            }
        },
        Cmd::Record(Some(target)) => {
            let clicks = uia_rec::click_collector(target);
            STATE.with(|s| s.borrow_mut().rec = Some(Recorder { target, events: vec![], last_move: (i32::MIN, 0), clicks, downs: 0 }))
        }
        Cmd::Record(None) => {
            if let Some(r) = STATE.with(|s| s.borrow_mut().rec.take()) {
                let events = clean_recording(r.events);
                // Emitted from this thread's loop once the pending clicks are resolved.
                r.clicks.finish(move |got| {
                    let els = uia_rec::rec_elements(&events, got);
                    FINISHED.lock().unwrap_or_else(|e| e.into_inner()).push((events, els));
                    wake_thread();
                });
            }
        }
        Cmd::Pick => STATE.with(|s| s.borrow_mut().picking = true),
    }
}

fn ensure_hooks() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let want = s.rec.is_some() || s.picking || s.swallow_up || FOCUS_RUNS.load(Ordering::SeqCst) > 0;
        unsafe {
            match (&s.hooks, want) {
                (None, true) => {
                    let hmod = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                    match (SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0), SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), hmod, 0)) {
                        (Ok(m), Ok(k)) => s.hooks = Some((m, k)),
                        (m, k) => {
                            for h in [m, k].into_iter().flatten() {
                                let _ = UnhookWindowsHookEx(h);
                            }
                            drop(s);
                            emit(Out::Error("Falha ao instalar os hooks de mouse/teclado".into()));
                        }
                    }
                }
                (Some((m, k)), false) => {
                    let _ = UnhookWindowsHookEx(*m);
                    let _ = UnhookWindowsHookEx(*k);
                    s.hooks = None;
                }
                _ => {}
            }
        }
    });
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let m = &*(lp.0 as *const MSLLHOOKSTRUCT);
            if m.flags & LLMHF_INJECTED == 0 {
                LAST_PHYSICAL.store(tick(), Ordering::Relaxed);
                if on_mouse(wp.0 as u32, m) {
                    return LRESULT(1); // swallowed (pick click)
                }
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// Returns true when the event must be swallowed.
fn on_mouse(msg: u32, m: &MSLLHOOKSTRUCT) -> bool {
    let (x, y) = (m.pt.x, m.pt.y);
    let picked = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.picking && msg == WM_LBUTTONDOWN {
            s.picking = false;
            s.swallow_up = true;
            return Some(true);
        }
        if s.swallow_up && msg == WM_LBUTTONUP {
            s.swallow_up = false;
            return Some(false);
        }
        let Some(r) = s.rec.as_mut() else { return None };
        // While a button is held we keep recording even outside the window (drags).
        let held = r.events.iter().rev().find_map(|e| match e.ev {
            Ev::Button(_, d, _, _) => Some(d),
            _ => None,
        }) == Some(true);
        if win::root_at(x, y) != r.target && !held {
            return None;
        }
        let (cx, cy) = win::screen_to_client(r.target, x, y);
        let btn = |b| Some((b, matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)));
        let ev = match msg {
            WM_MOUSEMOVE if r.last_move != (cx, cy) => {
                r.last_move = (cx, cy);
                Some(Ev::Move(cx, cy))
            }
            WM_MOUSEWHEEL => Some(Ev::Wheel((m.mouseData >> 16) as i16 as i32, cx, cy)),
            WM_LBUTTONDOWN | WM_LBUTTONUP => btn(Btn::Left).map(|(b, d)| Ev::Button(b, d, cx, cy)),
            WM_RBUTTONDOWN | WM_RBUTTONUP => btn(Btn::Right).map(|(b, d)| Ev::Button(b, d, cx, cy)),
            WM_MBUTTONDOWN | WM_MBUTTONUP => btn(Btn::Middle).map(|(b, d)| Ev::Button(b, d, cx, cy)),
            _ => None,
        };
        if let Some(ev) = ev {
            if let Ev::Button(_, true, cx, cy) = ev {
                // Only a channel send: the element is resolved on the collector's thread.
                r.clicks.click(Click { ord: r.downs, sx: x, sy: y, cx, cy });
                r.downs += 1;
            }
            r.events.push(Event { t: m.time, ev });
        }
        None
    });
    match picked {
        Some(true) => {
            emit(Out::Picked { x, y, root: win::root_at(x, y) });
            true
        }
        Some(false) => {
            wake_thread(); // pick finished: let the loop uninstall the hooks
            true
        }
        None => false,
    }
}

unsafe extern "system" fn kbd_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
            if !k.flags.contains(LLKHF_INJECTED) {
                let down = !k.flags.contains(LLKHF_UP);
                if down {
                    // Key-ups don't count: releasing a hotkey must not pause a Focus run.
                    LAST_PHYSICAL.store(tick(), Ordering::Relaxed);
                }
                STATE.with(|s| {
                    if let Some(r) = s.borrow_mut().rec.as_mut() {
                        if win::is_foreground(r.target) {
                            let ev = Ev::Key(k.vkCode as u16, k.scanCode as u16, k.flags.contains(LLKHF_EXTENDED), down);
                            r.events.push(Event { t: k.time, ev });
                        }
                    }
                });
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// Makes timestamps start at 0 and drops hotkey noise: key-ups at the start for keys never
/// pressed during the recording (the start chord being released), and the trailing key-downs
/// (the stop chord — recording ends on its last key-down, before any of its key-ups).
pub fn clean_recording(mut ev: Vec<Event>) -> Vec<Event> {
    let mut pressed: Vec<u16> = vec![];
    ev.retain(|e| match e.ev {
        Ev::Key(vk, _, _, true) => {
            pressed.push(vk);
            true
        }
        Ev::Key(vk, _, _, false) => pressed.contains(&vk),
        _ => true,
    });
    while matches!(ev.last(), Some(Event { ev: Ev::Key(_, _, _, true), .. })) {
        ev.pop();
    }
    if let Some(t0) = ev.first().map(|e| e.t) {
        for e in &mut ev {
            e.t = e.t.wrapping_sub(t0);
        }
    }
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(t: u32, vk: u16, down: bool) -> Event {
        Event { t, ev: Ev::Key(vk, 0, false, down) }
    }

    #[test]
    fn cleans_hotkey_noise() {
        let ev = vec![
            k(100, 0x11, false), // Ctrl up from the start hotkey
            k(150, b'R' as u16, false),
            Event { t: 200, ev: Ev::Move(1, 2) },
            k(300, b'A' as u16, true),
            k(350, b'A' as u16, false),
            k(900, 0xA2, true), // stop hotkey: LCtrl, LShift, LAlt, R
            k(910, 0xA0, true),
            k(920, 0xA4, true),
            k(930, b'R' as u16, true),
        ];
        let out = clean_recording(ev);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], Event { t: 0, ev: Ev::Move(1, 2) });
        assert_eq!(out[2].t, 150);
        // A recording that ends in a completed key press keeps it.
        let out = clean_recording(vec![k(0, b'A' as u16, true), k(10, b'A' as u16, false)]);
        assert_eq!(out.len(), 2);
    }
}
