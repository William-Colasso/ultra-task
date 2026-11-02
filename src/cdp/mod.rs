//! "Navegador (CDP)" mode: one more `Io` for the interpreter. UltraTask opens a Chromium,
//! Electron or WebView2 app with remote debugging (own port and disposable profile per run),
//! picks the page by the target rule's title and drives it through the Chrome DevTools
//! Protocol: no real mouse/keyboard, works with the window covered, runs in parallel.
//!
//! - `client`: discovery + JSON-RPC over WebSocket (127.0.0.1, no TLS);
//! - `input`: blocks' events → `Input.*` commands, client-area → viewport coordinates;
//! - `launch`: command line / environment of the app;
//! - this file: the `Io`, the run thread and the Windows glue (window lookup, OCR).
pub mod client;
pub mod input;
pub mod launch;
pub mod ui;

pub use launch::validate;

use crate::model::{CdpLaunch, Ev, Region, Script, Settings, TargetRule};
use crate::runner::{Debug, Halt, Interp, Io, RunHandle};
use crate::win::{self, Hwnd, Image, WinInfo};
use client::{Conn, Target};
use input::{Geometry, InputState};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the app gets to open and show the page.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);
/// Each CDP request; a screenshot of a big window can take a while.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Window size and devicePixelRatio are re-read at most this often.
const GEOMETRY_MS: u128 = 500;

/// Best window of the app for coordinate conversion: the launched process's own window,
/// else one matching the target rule; among several, the one showing the page's title.
pub fn choose_window(wins: &[WinInfo], pid: Option<u32>, rule: Option<&TargetRule>, page_title: &str) -> Option<Hwnd> {
    let pick = |c: Vec<&WinInfo>| c.iter().find(|w| !page_title.is_empty() && w.title.contains(page_title)).or(c.first()).map(|w| w.hwnd);
    let own: Vec<&WinInfo> = wins.iter().filter(|w| Some(w.pid) == pid).collect();
    if !own.is_empty() {
        return pick(own);
    }
    pick(wins.iter().filter(|w| rule.is_some_and(|r| r.matches(&w.exe, &w.title))).collect())
}

/// Base64 (standard alphabet, padding optional) → bytes.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for &c in s.as_bytes().iter().filter(|c| !c.is_ascii_whitespace() && **c != b'=') {
        acc = (acc << 6) | val(c).ok_or("base64 inválido")? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// PNG from `Page.captureScreenshot` → BGRA image.
pub fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let e = |e: png::DecodingError| format!("PNG: {e}");
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(e)?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("PNG grande demais")?];
    let info = r.next_frame(&mut buf).map_err(e)?;
    let n = info.color_type.samples();
    let mut bgra = Vec::with_capacity((info.width * info.height * 4) as usize);
    for row in buf.chunks(info.line_size).take(info.height as usize) {
        for px in row[..info.width as usize * n].chunks(n) {
            let (r, g, b) = if n >= 3 { (px[0], px[1], px[2]) } else { (px[0], px[0], px[0]) };
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    Ok(Image { w: info.width as i32, h: info.height as i32, bgra })
}

/// Live connection to the page of one run.
pub struct CdpIo {
    conn: Conn,
    pid: Option<u32>,
    rule: Option<TargetRule>,
    page_title: String,
    hwnd: Option<Hwnd>,
    input: InputState,
    geom: Geometry,
    /// Client-area size of the window (or the viewport, in physical px, without a window).
    client: (i32, i32),
    geom_at: Option<Instant>,
    start: Instant,
    ocr_scale: u32,
}

impl CdpIo {
    fn new(conn: Conn, pid: Option<u32>, rule: Option<TargetRule>, page_title: String, ocr_scale: u32) -> Self {
        CdpIo { conn, pid, rule, page_title, hwnd: None, input: InputState::default(), geom: Geometry::default(), client: (0, 0), geom_at: None, start: Instant::now(), ocr_scale }
    }

    fn call(&mut self, method: &'static str, params: Value) -> Result<Value, Halt> {
        self.conn.call(method, params).map_err(|e| Halt::Error(format!("CDP {e}")))
    }

    fn geometry(&mut self) -> Geometry {
        if self.geom_at.is_some_and(|t| t.elapsed().as_millis() < GEOMETRY_MS) {
            return self.geom;
        }
        if self.hwnd.is_none_or(|h| !win::is_alive(h)) {
            self.hwnd = choose_window(&win::list_windows(), self.pid, self.rule.as_ref(), &self.page_title);
        }
        if let Ok(v) = self.conn.eval("[innerWidth, innerHeight, devicePixelRatio]") {
            let f = |i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
            let client = self.hwnd.and_then(win::client_size);
            self.geom = Geometry::new(client, (f(0), f(1)), f(2));
            self.client = client.unwrap_or(((f(0) * self.geom.dpr) as i32, (f(1) * self.geom.dpr) as i32));
        }
        self.geom_at = Some(Instant::now());
        self.geom
    }

    fn screenshot(&mut self) -> Result<Image, Halt> {
        let r = self.call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = r.get("data").and_then(Value::as_str).ok_or("CDP: captura sem dados")?;
        Ok(decode_png(&base64_decode(data)?)?)
    }

    fn release_all(&mut self) {
        let g = self.geom;
        let now = self.now_ms();
        for (m, p) in self.input.release_all(now, &g) {
            let _ = self.conn.call(m, p);
        }
    }
}

impl Io for CdpIo {
    fn send(&mut self, ev: &Ev) -> Result<(), Halt> {
        let g = self.geometry();
        let now = self.now_ms();
        for (m, p) in self.input.translate(ev, now, &g) {
            self.call(m, p)?;
        }
        Ok(())
    }
    fn type_text(&mut self, s: &str) -> Result<(), Halt> {
        let (m, p) = input::insert_text(s);
        self.call(m, p).map(|_| ())
    }
    fn pixel(&mut self, x: i32, y: i32) -> Result<[u8; 3], Halt> {
        let g = self.geometry();
        let (sx, sy) = g.to_shot(x, y);
        self.screenshot()?.pixel(sx, sy).ok_or_else(|| Halt::Error(format!("pixel ({x},{y}) fora da página")))
    }
    fn ocr(&mut self, r: Region) -> Result<String, Halt> {
        let g = self.geometry();
        let (x, y) = g.to_shot(r.x, r.y);
        let img = self.screenshot()?.crop(Region { x, y, ..r });
        Ok(win::ocr(&img.scale(self.ocr_scale))?)
    }
    fn window_exists(&mut self) -> bool {
        self.conn.eval("1").is_ok()
    }
    fn window_foreground(&mut self) -> bool {
        self.geometry();
        self.hwnd.is_some_and(win::is_foreground)
    }
    fn window_title(&mut self) -> String {
        match self.conn.eval("document.title") {
            Ok(Value::String(t)) => {
                self.page_title = t.clone();
                t
            }
            _ => String::new(),
        }
    }
    fn now_ms(&mut self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    fn sleep(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
    /// Start of a human-mouse path: there is no real cursor, so the last point sent, or the
    /// middle of the page before the first one.
    fn pointer(&mut self, last_sent: Option<(i32, i32)>) -> (i32, i32) {
        last_sent.unwrap_or_else(|| {
            let g = self.geometry();
            (g.off_x + (self.client.0 - g.off_x) / 2, g.off_y + (self.client.1 - g.off_y) / 2)
        })
    }
    fn uia(&mut self, _q: &crate::uia::Query) -> Result<crate::uia::Reply, Halt> {
        Err(UIA_UNAVAILABLE.into())
    }
}

/// UI Automation needs the window's accessibility tree, which this mode doesn't use.
pub const UIA_UNAVAILABLE: &str = "UI Automation não funciona no modo Navegador (CDP); use Clique, Pixel ou OCR";

/// The app opened for one run (none when attaching to an app already open).
struct Opened {
    child: Child,
    profile: PathBuf,
}

/// Opens the app (if `exe` is set), waits for a page matching the target rule and connects.
fn open(script: &Script, c: &CdpLaunch, ocr_scale: u32, stop: &AtomicBool, set: &dyn Fn(&str)) -> Result<(CdpIo, Option<Opened>, u16), (String, Option<Opened>)> {
    let port = match c.port {
        0 if !c.exe.trim().is_empty() => launch::free_port().map_err(|e| (e, None))?,
        p => p,
    };
    let mut opened = None;
    if !c.exe.trim().is_empty() {
        let profile = launch::profile_dir(&script.name, port);
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).map_err(|e| (format!("perfil {}: {e}", profile.display()), None))?;
        let plan = launch::plan(c, port, &profile).map_err(|e| (e, None))?;
        set(&format!("Abrindo o app (porta {port})"));
        let child = plan.spawn().map_err(|e| (e, None))?;
        opened = Some(Opened { child, profile });
    }
    let fail = |e: String, o: Option<Opened>| Err((e, o));
    let title = script.target.as_ref().map(|r| r.title.as_str());
    set(&format!("Procurando a página em 127.0.0.1:{port}"));
    let t0 = Instant::now();
    let mut last_err;
    let page: Target = loop {
        if stop.load(Ordering::Relaxed) {
            return fail(String::new(), opened);
        }
        if let Some(o) = &mut opened
            && let Ok(Some(st)) = o.child.try_wait()
        {
            return fail(format!("o app fechou ao abrir ({st}). Outro processo dele já estava aberto? Apps de instância única passam o pedido adiante e saem."), opened);
        }
        match client::list_targets(port) {
            Ok(ts) => match client::pick_page(&ts, title) {
                Some(p) => break p.clone(),
                None => last_err = format!("nenhuma página com título \"{}\" (há: {})", title.unwrap_or("*"), ts.iter().filter(|t| t.kind == "page").map(|t| format!("\"{}\"", t.title)).collect::<Vec<_>>().join(", ")),
            },
            Err(e) => last_err = e,
        }
        if t0.elapsed() > OPEN_TIMEOUT {
            return fail(format!("o app não abriu a depuração em {} s: {last_err}", OPEN_TIMEOUT.as_secs()), opened);
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let path = client::ws_path(&page.ws_url).unwrap_or_default().to_string();
    let mut conn = match Conn::connect(port, &path, CALL_TIMEOUT) {
        Ok(c) => c,
        Err(e) => return fail(e, opened),
    };
    // The page believes it has focus (focus events, :focus, document.hasFocus()) while covered.
    let _ = conn.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true }));
    let pid = opened.as_ref().map(|o| o.child.id());
    Ok((CdpIo::new(conn, pid, script.target.clone(), page.title, ocr_scale), opened, port))
}

/// Closes the app we opened and deletes its temporary profile, unless `keep_open`.
fn close(io: Option<&mut CdpIo>, opened: Option<Opened>, keep_open: bool, webview2: bool, say: &dyn Fn(String)) {
    let Some(mut o) = opened else { return };
    if keep_open {
        say(format!("o app continua aberto; perfil temporário em {}", o.profile.display()));
        return;
    }
    if let Some(io) = io
        && !webview2
    {
        let _ = io.conn.call("Browser.close", json!({}));
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) && matches!(o.child.try_wait(), Ok(None)) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = o.child.kill();
    let _ = o.child.wait();
    // Helper processes may hold profile files for a moment after the main one exits.
    let t0 = Instant::now();
    while std::fs::remove_dir_all(&o.profile).is_err() && o.profile.exists() {
        if t0.elapsed() > Duration::from_secs(10) {
            say(format!("aviso: não deu para apagar o perfil temporário {}", o.profile.display()));
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Run thread of a CDP script (the counterpart of `runner::spawn`).
pub fn spawn(script: Script, all: Vec<Script>, settings: &Settings, delay_ms: u64, log: Sender<String>, wake: Arc<dyn Fn() + Send + Sync>) -> RunHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("Iniciando".to_string()));
    let done = Arc::new(AtomicBool::new(false));
    let dbg = Arc::new(Debug::default());
    let cfg = script.cdp.clone().unwrap_or_default();
    let h = RunHandle {
        script: script.name.clone(),
        window: format!("CDP {}", script.target.as_ref().map_or("*", |t| t.title.as_str())),
        stop: stop.clone(),
        status: status.clone(),
        done: done.clone(),
        dbg: dbg.clone(),
    };
    let ocr_scale = settings.ocr_scale;
    std::thread::spawn(move || {
        let say = |m: String| {
            let _ = log.send(format!("[{}] {m}", script.name));
            wake();
        };
        let set = |m: &str| {
            *status.lock().unwrap() = m.into();
            wake();
        };
        if delay_ms > 0 {
            set(&format!("Aguardando {:.1} s (gatilho)", delay_ms as f32 / 1000.0));
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(delay_ms) && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let (result, mut io, opened) = if stop.load(Ordering::Relaxed) {
            (Err(Halt::Stopped), None, None)
        } else {
            match open(&script, &cfg, ocr_scale, &stop, &set) {
                Err((_, o)) if stop.load(Ordering::Relaxed) => (Err(Halt::Stopped), None, o),
                Err((e, o)) => (Err(Halt::Error(e)), None, o),
                Ok((mut io, o, port)) => {
                    say(format!("conectado a \"{}\" em 127.0.0.1:{port}", io.page_title));
                    let mut log_fn = |m: String| say(m);
                    let r = {
                        let mut it = Interp::new(&mut io, &stop, &all, script.speed, &mut log_fn, &dbg);
                        it.trace = script.trace;
                        it.human_mouse = script.human_mouse;
                        it.start_integrations(&script.integrations);
                        let mut n = 0u32;
                        let r = loop {
                            n += 1;
                            set(&if script.repeat == 1 { "Rodando".to_string() } else { format!("Rodando (volta {n})") });
                            if let Err(h) = it.run_blocks(&script.blocks) {
                                break Err(h);
                            }
                            if script.repeat != 0 && n >= script.repeat {
                                break Ok(());
                            }
                        };
                        if script.integrations.iter().any(|i| i.when == crate::integrations::When::OnEnd) {
                            set("Aguardando integrações (ao terminar)");
                        }
                        it.finish_integrations();
                        r
                    };
                    io.release_all();
                    (r, Some(io), o)
                }
            }
        };
        close(io.as_mut(), opened, cfg.keep_open, cfg.webview2, &say);
        match result {
            Ok(()) => say("terminou".into()),
            Err(Halt::Stopped) => say("parado".into()),
            Err(Halt::Error(e)) => say(format!("erro: {e}")),
        }
        set("Terminado");
        done.store(true, Ordering::SeqCst);
        wake();
    });
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(hwnd: Hwnd, pid: u32, exe: &str, title: &str) -> WinInfo {
        WinInfo { hwnd, title: title.into(), exe: exe.into(), pid }
    }

    #[test]
    fn window_of_the_launched_process_wins() {
        let wins = [
            w(1, 10, "chrome.exe", "Outra - Google Chrome"),
            w(2, 20, "chrome.exe", "Painel - Google Chrome"),
            w(3, 20, "chrome.exe", "Caixa de entrada - Google Chrome"),
            w(4, 30, "app.exe", "Caixa de entrada"),
        ];
        let rule = TargetRule { exe: "app.exe".into(), title: "*".into() };
        assert_eq!(choose_window(&wins, Some(20), Some(&rule), "Caixa de entrada"), Some(3));
        assert_eq!(choose_window(&wins, Some(20), None, "Nada"), Some(2));
        // Launcher stubs: the window belongs to another process, found by the rule.
        assert_eq!(choose_window(&wins, Some(99), Some(&rule), "Caixa de entrada"), Some(4));
        assert_eq!(choose_window(&wins, None, None, "Caixa de entrada"), None);
    }

    #[test]
    fn script_json_and_validator() {
        let plain = Script { name: "a".into(), ..Default::default() };
        assert!(!serde_json::to_string(&plain).unwrap().contains("cdp"), "outros modos não ganham o campo");
        let s: Script = serde_json::from_str(r#"{"name":"b","cdp":{"exe":"chrome.exe"},"trigger":{"OnWindowOpen":{"delay_s":1.0}},"target":{"exe":"chrome.exe","title":"*"}}"#).unwrap();
        assert_eq!(s.cdp, Some(CdpLaunch { exe: "chrome.exe".into(), ..Default::default() }));
        assert_eq!(serde_json::from_str::<Script>(&serde_json::to_string(&s).unwrap()).unwrap(), s);
        let errs = crate::model::validate(&s, &[]);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].starts_with("trigger: no modo CDP"), "{errs:?}");
        assert!(serde_json::from_str::<Script>(r#"{"name":"c","cdp":{"exe":"x","porta":1}}"#).is_err());
    }

    #[test]
    fn base64() {
        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
        assert_eq!(base64_decode("TWE=").unwrap(), b"Ma");
        assert_eq!(base64_decode("TQ==\n").unwrap(), b"M");
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("T$==").is_err());
    }

    #[test]
    fn screenshot_png_to_bgra() {
        let mut png_bytes = vec![];
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 10, 20, 30, 128]).unwrap();
        }
        let img = decode_png(&png_bytes).unwrap();
        assert_eq!((img.w, img.h), (2, 2));
        assert_eq!(img.pixel(0, 0), Some([255, 0, 0]));
        assert_eq!(img.pixel(1, 0), Some([0, 255, 0]));
        assert_eq!(img.pixel(0, 1), Some([0, 0, 255]));
        assert_eq!(img.pixel(1, 1), Some([10, 20, 30]));
        let mut rgb = vec![];
        {
            let mut enc = png::Encoder::new(&mut rgb, 1, 1);
            enc.set_color(png::ColorType::Rgb);
            enc.write_header().unwrap().write_image_data(&[1, 2, 3]).unwrap();
        }
        assert_eq!(decode_png(&rgb).unwrap().pixel(0, 0), Some([1, 2, 3]));
        assert!(decode_png(b"nada").is_err());
    }

    /// Against real Chromium instances already listening, e.g. on Linux, one per port:
    /// `chrome --headless=new --remote-debugging-port=9331 --user-data-dir=/tmp/p1 'data:text/html,<title>UT 1</title>'`,
    /// then `ULTRATASK_CDP_PORTS=9331,9332,9333 cargo test --target x86_64-pc-windows-gnu -- --ignored cdp_real`.
    /// Drives every instance at the same time through the interpreter and reads the result in the DOM.
    #[test]
    #[ignore = "precisa de Chromium aberto com --remote-debugging-port (ULTRATASK_CDP_PORTS=9331,9332)"]
    fn cdp_real_browser() {
        let ports: Vec<u16> = std::env::var("ULTRATASK_CDP_PORTS").expect("ULTRATASK_CDP_PORTS").split(',').map(|p| p.trim().parse().unwrap()).collect();
        let runs: Vec<_> = ports.into_iter().map(|port| std::thread::spawn(move || drive_real(port))).collect();
        for r in runs {
            r.join().unwrap();
        }
    }

    fn drive_real(port: u16) {
        use crate::model::{Block, Btn, ClickKind, CondKind, Condition, KeyAction, OnTimeout};
        let ts = client::list_targets(port).unwrap();
        let page = client::pick_page(&ts, Some("UT*")).expect("página com título UT*");
        let conn = Conn::connect(port, client::ws_path(&page.ws_url).unwrap(), CALL_TIMEOUT).unwrap();
        let mut io = CdpIo::new(conn, None, None, page.title.clone(), 1);
        io.conn.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true })).unwrap();
        io.conn
            .eval(
                "document.body.innerHTML = '<input id=i style=\"position:fixed;left:0;top:0;width:300px;height:40px\"><div id=b style=\"position:fixed;left:0;top:100px;width:50px;height:50px;background:rgb(200,30,60)\"></div>'; \
                 window.log = []; addEventListener('keydown', e => { if (e.ctrlKey && e.key === 's') { e.preventDefault(); log.push('ctrl+s'); } }); \
                 addEventListener('dblclick', e => log.push('dbl ' + e.clientX + ',' + e.clientY)); \
                 window.moves = 0; addEventListener('mousemove', () => moves++); 1",
            )
            .unwrap();
        let dpr = io.geometry().dpr;
        let px = |v: i32| ((v as f64 * dpr) as i32).to_string();
        let key = |vk: u16, action: KeyAction| Block::Key { vk, action };
        let blocks = vec![
            Block::Click { x: px(20), y: px(20), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 },
            Block::Type { text: format!("olá {port} ") },
            key(0x10, KeyAction::Down),
            key(0x31, KeyAction::Press),
            key(0x10, KeyAction::Up),
            key(0x11, KeyAction::Down),
            key(0x53, KeyAction::Press),
            key(0x11, KeyAction::Up),
            Block::Click { x: px(300), y: px(300), button: Btn::Left, kind: ClickKind::Double, jitter_px: 0 },
            Block::WaitUntil {
                cond: Condition { kind: CondKind::Pixel { x: px(25), y: px(125), color: [200, 30, 60], tol: 2 }, negate: false },
                timeout_ms: "3000".into(),
                on_timeout: OnTimeout::Stop,
            },
        ];
        let (stop, dbg) = (AtomicBool::new(false), Debug::default());
        let mut logs = |m: String| eprintln!("{port}: {m}");
        {
            // Human mouse on: the path's many Moves become mouseMoved and the clicks still land.
            let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut logs, &dbg);
            it.human_mouse = true;
            it.run_blocks(&blocks).unwrap();
            let uia = Block::Uia { elem: crate::uia::UiaElem { name: "x".into(), ..Default::default() }, action: crate::uia::UiaAction::Invoke, timeout_ms: "0".into(), fallback_click: false };
            assert_eq!(it.run_blocks(&[uia]), Err(Halt::Error(UIA_UNAVAILABLE.into())));
        }
        assert!(io.conn.eval("moves").unwrap().as_u64().unwrap() > 4, "trajeto humano gera vários mouseMoved");
        assert_eq!(io.conn.eval("document.getElementById('i').value").unwrap(), json!(format!("olá {port} !")));
        assert_eq!(io.conn.eval("log").unwrap(), json!(["ctrl+s", "dbl 300,300"]));
        assert_eq!(io.pixel(1, 1).unwrap(), [255, 255, 255], "dentro do input");
        assert_eq!(io.window_title(), page.title);
        assert!(io.window_exists());
    }
}
