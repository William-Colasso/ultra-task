//! Block interpreter + run threads. All OS access goes through `Io`, so the control flow
//! is testable with a fake clock.
use crate::hooks;
use crate::humanmouse;
use crate::model::*;
use crate::win::{self, Injector, WinInfo};
use evalexpr::{ContextWithMutableFunctions, ContextWithMutableVariables, EvalexprResult, Function, HashMapContext, IterateVariablesContext, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum Halt {
    Stopped,
    Error(String),
}

impl From<String> for Halt {
    fn from(s: String) -> Self {
        Halt::Error(s)
    }
}

impl From<&str> for Halt {
    fn from(s: &str) -> Self {
        Halt::Error(s.into())
    }
}

type R<T = ()> = Result<T, Halt>;

pub trait Io {
    fn send(&mut self, ev: &Ev) -> R;
    fn type_text(&mut self, s: &str) -> R;
    fn pixel(&mut self, x: i32, y: i32) -> R<[u8; 3]>;
    fn ocr(&mut self, r: Region) -> R<String>;
    fn window_exists(&mut self) -> bool;
    fn window_foreground(&mut self) -> bool;
    fn window_title(&mut self) -> String;
    fn now_ms(&mut self) -> u64;
    fn sleep(&mut self, ms: u64);
    /// Ponto de partida do movimento humano do mouse (coordenadas de cliente).
    /// `last_sent` é a última posição que a execução enviou.
    fn pointer(&mut self, last_sent: Option<(i32, i32)>) -> (i32, i32) {
        last_sent.unwrap_or((0, 0))
    }
}

/// Live view of a run for the debug panel; shared between the run thread and the UI.
#[derive(Default)]
pub struct Debug {
    pub vars: Mutex<Vec<(String, String)>>,
    pub current: Mutex<String>,
    /// Set by a Breakpoint block or the UI's pause button; the run waits between blocks.
    pub paused: AtomicBool,
}

pub struct Interp<'a> {
    io: &'a mut dyn Io,
    vars: HashMapContext,
    stop: &'a AtomicBool,
    scripts: &'a [Script],
    speed: f32,
    log: &'a mut dyn FnMut(String),
    dbg: &'a Debug,
    pub trace: bool,
    /// "Movimento humano do mouse": desloca o ponteiro por um trajeto em vez de teletransportar.
    pub human_mouse: bool,
    /// Última posição do mouse enviada por esta execução.
    pos: Option<(i32, i32)>,
    depth: u32,
    last_warn: String,
    /// Integrações do script de cima (as de scripts chamados com Chamar não entram).
    integ: Option<crate::integrations::engine::Engine>,
}

fn jit(v: i64, j: u32) -> i64 {
    if j == 0 { v } else { v + fastrand::i64(-(j as i64)..=j as i64) }
}

/// Funções que o UltraTask acrescenta ao evalexpr. Toda função nova entra aqui e em
/// `catalog::FUNCTIONS` (um teste confere).
pub fn custom_functions() -> Vec<(&'static str, Function<evalexpr::DefaultNumericTypes>)> {
    vec![
        (
            "rand",
            Function::new(|a: &Value| -> EvalexprResult<Value> {
                let t = a.as_fixed_len_tuple(2)?;
                let (x, y): (i64, i64) = (t[0].as_int()?, t[1].as_int()?);
                Ok(Value::Int(fastrand::i64(x.min(y)..=x.max(y))))
            }),
        ),
        (
            "contem",
            Function::new(|a: &Value| -> EvalexprResult<Value> {
                let t = a.as_fixed_len_tuple(2)?;
                Ok(Value::Boolean(t[0].as_string()?.to_lowercase().contains(&t[1].as_string()?.to_lowercase())))
            }),
        ),
        (
            "json",
            Function::new(|a: &Value| -> EvalexprResult<Value> {
                let t = a.as_fixed_len_tuple(2)?;
                crate::integrations::json_get(&t[0].as_string()?, &t[1].as_string()?).map_err(evalexpr::EvalexprError::CustomMessage)
            }),
        ),
    ]
}

/// Contexto de expressões novo, sem variáveis, com as funções do UltraTask.
pub fn new_context() -> HashMapContext {
    let mut vars = HashMapContext::new();
    for (name, f) in custom_functions() {
        let _ = vars.set_function(name.into(), f);
    }
    vars
}

impl<'a> Interp<'a> {
    pub fn new(io: &'a mut dyn Io, stop: &'a AtomicBool, scripts: &'a [Script], speed: f32, log: &'a mut dyn FnMut(String), dbg: &'a Debug) -> Self {
        let vars = new_context();
        Interp { io, vars, stop, scripts, speed, log, dbg, trace: false, human_mouse: false, pos: None, depth: 0, last_warn: String::new(), integ: None }
    }

    /// Liga as integrações da execução e cria as variáveis de resultado.
    pub fn start_integrations(&mut self, list: &[crate::integrations::Integration]) {
        if list.is_empty() {
            return;
        }
        let now = self.io.now_ms();
        self.integ = Some(crate::integrations::engine::Engine::new(list, &mut self.vars, now));
        self.snapshot_vars();
    }

    /// Respostas que chegaram + gatilhos "a cada N s".
    fn pump_integrations(&mut self) {
        let Some(e) = self.integ.as_mut() else { return };
        let now = self.io.now_ms();
        if e.pump(&mut self.vars, now, &mut *self.log) {
            self.snapshot_vars();
        }
    }

    fn var_set(&mut self, name: &str) {
        if let Some(e) = self.integ.as_mut() {
            e.on_set(name.trim(), &mut self.vars, &mut *self.log);
        }
    }

    /// Gatilhos "ao terminar"; não roda se o usuário parou (Parar descarta as respostas).
    pub fn finish_integrations(&mut self) {
        if self.stop.load(Ordering::Relaxed) {
            return;
        }
        if let Some(e) = self.integ.as_mut()
            && e.finish(&mut self.vars, &mut *self.log, self.stop)
        {
            self.snapshot_vars();
        }
    }

    fn snapshot_vars(&self) {
        let mut v: Vec<(String, String)> = self
            .vars
            .iter_variables()
            .map(|(k, v)| {
                let shown = match v {
                    Value::String(s) => format!("\"{s}\""),
                    v => v.to_string(),
                };
                (k, shown)
            })
            .collect();
        v.sort();
        *self.dbg.vars.lock().unwrap() = v;
    }

    /// Blocks while the run is paused (breakpoint / pause button); returns the time spent.
    fn wait_if_paused(&mut self) -> R<u64> {
        if !self.dbg.paused.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let t0 = self.io.now_ms();
        while self.dbg.paused.load(Ordering::Relaxed) {
            self.check()?;
            self.pump_integrations();
            self.io.sleep(50);
        }
        Ok(self.io.now_ms() - t0)
    }

    fn interp_text(&mut self, text: &str) -> R<String> {
        Ok(interpolate(text, |e| {
            self.text(e).map_err(|h| match h {
                Halt::Error(s) => s,
                Halt::Stopped => String::new(),
            })
        })?)
    }

    fn warn(&mut self, m: String) {
        // Conditions poll every 100 ms; don't flood the log with the same failure.
        if m != self.last_warn {
            (self.log)(m.clone());
            self.last_warn = m;
        }
    }

    fn eval(&mut self, s: &str) -> R<Value> {
        evalexpr::eval_with_context_mut(s.trim(), &mut self.vars).map_err(|e| Halt::Error(format!("expressão '{s}': {e}")))
    }

    fn int(&mut self, s: &str) -> R<i64> {
        match self.eval(s)? {
            Value::Int(i) => Ok(i),
            Value::Float(f) => Ok(f.round() as i64),
            v => Err(Halt::Error(format!("'{s}' deveria ser um número, deu {v}"))),
        }
    }

    fn text(&mut self, s: &str) -> R<String> {
        Ok(match self.eval(s)? {
            Value::String(s) => s,
            v => v.to_string(),
        })
    }

    fn check(&self) -> R {
        if self.stop.load(Ordering::Relaxed) { Err(Halt::Stopped) } else { Ok(()) }
    }

    fn sleep_until(&mut self, end: u64) -> R {
        loop {
            self.check()?;
            self.pump_integrations();
            let now = self.io.now_ms();
            if now >= end {
                return Ok(());
            }
            self.io.sleep((end - now).min(20));
        }
    }

    fn sleep(&mut self, ms: u64) -> R {
        let end = self.io.now_ms() + ms;
        self.sleep_until(end)
    }

    fn send(&mut self, ev: &Ev) -> R {
        if let Ev::Move(x, y) | Ev::Button(_, _, x, y) | Ev::Wheel(_, x, y) = *ev {
            self.pos = Some((x, y));
        }
        self.io.send(ev)
    }

    /// Leva o ponteiro até (x, y): direto, ou pelo trajeto humano quando a opção está ligada.
    /// O trajeto respeita a velocidade do script, parar e pausar.
    fn move_to(&mut self, x: i32, y: i32) -> R {
        if !self.human_mouse {
            return self.send(&Ev::Move(x, y));
        }
        let from = self.io.pointer(self.pos);
        let path = humanmouse::path(from, (x, y), fastrand::u64(..));
        let k = self.speed.max(0.01) as f64;
        let mut base = self.io.now_ms();
        for (i, &(px, py)) in path.points.iter().enumerate() {
            base += self.wait_if_paused()?;
            self.sleep_until(base + (((i as u64 + 1) * humanmouse::STEP_MS) as f64 / k) as u64)?;
            self.send(&Ev::Move(px, py))?;
        }
        Ok(())
    }

    pub fn run_blocks(&mut self, blocks: &[Block]) -> R {
        for b in blocks {
            self.check()?;
            self.pump_integrations();
            self.wait_if_paused()?;
            let summary = b.summary();
            if self.trace {
                (self.log)(format!("» {summary}"));
            }
            *self.dbg.current.lock().unwrap() = summary;
            self.run_block(b)?;
        }
        Ok(())
    }

    fn set_var(&mut self, name: &str, v: Value) -> R {
        self.vars.set_value(name.trim().into(), v).map_err(|e| Halt::Error(format!("variável '{name}': {e}")))
    }

    fn run_block(&mut self, b: &Block) -> R {
        match b {
            Block::Recording { events, speed } => {
                let k = (speed * self.speed).max(0.01) as f64;
                let mut base = self.io.now_ms();
                for e in events {
                    base += self.wait_if_paused()?; // a pause shifts the rest of the timeline
                    self.sleep_until(base + (e.t as f64 / k) as u64)?;
                    self.send(&e.ev)?;
                }
            }
            Block::Move { x, y } => {
                let (x, y) = (self.int(x)? as i32, self.int(y)? as i32);
                self.move_to(x, y)?;
            }
            Block::Click { x, y, button, kind, jitter_px } => {
                let (x, y) = (jit(self.int(x)?, *jitter_px) as i32, jit(self.int(y)?, *jitter_px) as i32);
                let (down, up) = (Ev::Button(*button, true, x, y), Ev::Button(*button, false, x, y));
                self.move_to(x, y)?;
                match kind {
                    ClickKind::Click | ClickKind::Double => {
                        for _ in 0..if *kind == ClickKind::Double { 2 } else { 1 } {
                            self.send(&down)?;
                            self.sleep(30)?;
                            self.send(&up)?;
                            self.sleep(30)?;
                        }
                    }
                    ClickKind::Down => self.send(&down)?,
                    ClickKind::Up => self.send(&up)?,
                }
            }
            Block::Key { vk, action } => {
                let (d, u) = (Ev::Key(*vk, 0, false, true), Ev::Key(*vk, 0, false, false));
                match action {
                    KeyAction::Press => {
                        self.io.send(&d)?;
                        self.sleep(20)?;
                        self.io.send(&u)?;
                    }
                    KeyAction::Down => self.io.send(&d)?,
                    KeyAction::Up => self.io.send(&u)?,
                }
            }
            Block::Type { text } => {
                let s = self.interp_text(text)?;
                self.io.type_text(&s)?;
            }
            Block::Scroll { x, y, delta } => {
                let (x, y) = (self.int(x)? as i32, self.int(y)? as i32);
                if self.human_mouse {
                    self.move_to(x, y)?;
                }
                self.send(&Ev::Wheel(*delta, x, y))?;
            }
            Block::Wait { ms, jitter_ms } => {
                let ms = jit(self.int(ms)?, *jitter_ms).max(0);
                self.sleep(ms as u64)?;
            }
            Block::WaitUntil { cond, timeout_ms, on_timeout } => {
                let end = self.io.now_ms() + self.int(timeout_ms)?.max(0) as u64;
                while !self.cond(cond)? {
                    if self.io.now_ms() >= end {
                        match on_timeout {
                            OnTimeout::Stop => return Err("tempo esgotado em 'Esperar até'".into()),
                            OnTimeout::Continue => {
                                (self.log)("tempo esgotado em 'Esperar até' — continuando".into());
                                break;
                            }
                        }
                    }
                    self.sleep(100)?;
                }
            }
            Block::If { cond, then, else_ } => {
                if self.cond(cond)? {
                    self.run_blocks(then)?
                } else {
                    self.run_blocks(else_)?
                }
            }
            Block::Repeat { times, body } => {
                for _ in 0..self.int(times)?.max(0) {
                    self.run_blocks(body)?;
                    if body.is_empty() {
                        self.check()?;
                    }
                }
            }
            Block::While { cond, body } => {
                while self.cond(cond)? {
                    self.run_blocks(body)?;
                    if body.is_empty() {
                        self.sleep(50)?; // don't spin a core on an empty loop
                    }
                }
            }
            Block::SetVar { name, expr } => {
                let v = self.eval(expr)?;
                self.set_var(name, v)?;
                self.var_set(name);
                self.snapshot_vars();
            }
            Block::OcrToVar { name, region } => {
                let t = self.io.ocr(*region)?;
                self.set_var(name, Value::String(t.trim().into()))?;
                self.var_set(name);
                self.snapshot_vars();
            }
            Block::Log { text } => {
                let s = self.interp_text(text)?;
                (self.log)(format!("🐞 {s}"));
            }
            Block::Breakpoint => {
                self.snapshot_vars();
                (self.log)("⏸ breakpoint: veja as variáveis e clique ▶ na execução para continuar".into());
                self.dbg.paused.store(true, Ordering::SeqCst);
                self.wait_if_paused()?;
            }
            Block::Call { script } => {
                if self.depth >= 32 {
                    return Err("'Chamar script' passou de 32 níveis — recursão infinita?".into());
                }
                let scripts = self.scripts;
                let s = scripts.iter().find(|s| &s.name == script).ok_or_else(|| Halt::Error(format!("script '{script}' não existe")))?;
                self.depth += 1;
                let r = self.run_blocks(&s.blocks);
                self.depth -= 1;
                r?
            }
            Block::Stop => return Err(Halt::Stopped),
        }
        Ok(())
    }

    fn cond(&mut self, c: &Condition) -> R<bool> {
        let v = match &c.kind {
            CondKind::Pixel { x, y, color, tol } => {
                let (x, y) = (self.int(x)? as i32, self.int(y)? as i32);
                match self.io.pixel(x, y) {
                    Ok(p) => p.iter().zip(color).all(|(a, b)| a.abs_diff(*b) <= *tol),
                    Err(Halt::Error(e)) => {
                        self.warn(e);
                        false
                    }
                    Err(h) => return Err(h),
                }
            }
            CondKind::OcrContains { region, text } => match self.io.ocr(*region) {
                Ok(t) => t.to_lowercase().contains(&text.to_lowercase()),
                Err(Halt::Error(e)) => {
                    self.warn(e);
                    false
                }
                Err(h) => return Err(h),
            },
            CondKind::WindowExists => self.io.window_exists(),
            CondKind::WindowForeground => self.io.window_foreground(),
            CondKind::TitleMatches(p) => glob_match(p, &self.io.window_title()),
            CondKind::Expr(s) => match self.eval(s)? {
                Value::Boolean(b) => b,
                v => return Err(Halt::Error(format!("condição '{s}' deveria dar verdadeiro/falso, deu {v}"))),
            },
        };
        Ok(v != c.negate)
    }
}

/// Turns a raw recording into editable blocks. Mouse moves survive only while a button is
/// held (drags, sampled every 50 ms); presses held longer than 300 ms stay Down/Wait/Up.
pub fn explode(events: &[Event]) -> Vec<Block> {
    fn push(out: &mut Vec<Block>, gap: u32, b: Block) {
        if gap >= 20 {
            out.push(Block::Wait { ms: gap.to_string(), jitter_ms: 0 });
        }
        out.push(b);
    }
    let s = |v: i32| v.to_string();
    let mut out = vec![];
    let (mut last_t, mut held, mut down_t, mut last_move) = (0u32, false, 0u32, 0u32);
    let mut keys_down: Vec<(u16, u32)> = vec![];
    for e in events {
        let gap = e.t.saturating_sub(last_t);
        match e.ev {
            Ev::Move(x, y) => {
                if held && e.t.saturating_sub(last_move) >= 50 {
                    push(&mut out, gap, Block::Move { x: s(x), y: s(y) });
                    (last_t, last_move) = (e.t, e.t);
                }
            }
            Ev::Button(b, true, x, y) => {
                push(&mut out, gap, Block::Click { x: s(x), y: s(y), button: b, kind: ClickKind::Down, jitter_px: 0 });
                (last_t, held, down_t, last_move) = (e.t, true, e.t, e.t);
            }
            Ev::Button(b, false, x, y) => {
                held = false;
                let quick = e.t.saturating_sub(down_t) <= 300;
                if let Some(Block::Click { kind: k @ ClickKind::Down, button, x: px, y: py, .. }) = out.last_mut()
                    && quick
                    && *button == b
                    && *px == s(x)
                    && *py == s(y)
                {
                    *k = ClickKind::Click;
                    merge_double(&mut out);
                } else {
                    push(&mut out, gap, Block::Click { x: s(x), y: s(y), button: b, kind: ClickKind::Up, jitter_px: 0 });
                }
                last_t = e.t;
            }
            Ev::Wheel(d, x, y) => {
                push(&mut out, gap, Block::Scroll { x: s(x), y: s(y), delta: d });
                last_t = e.t;
            }
            Ev::Key(vk, _, _, true) => {
                // Autorepeat downs of a key already held are dropped.
                if !keys_down.iter().any(|k| k.0 == vk) {
                    push(&mut out, gap, Block::Key { vk, action: KeyAction::Down });
                    keys_down.push((vk, e.t));
                    last_t = e.t;
                }
            }
            Ev::Key(vk, _, _, false) => {
                let t_down = keys_down.iter().find(|k| k.0 == vk).map(|k| k.1);
                keys_down.retain(|k| k.0 != vk);
                let quick = t_down.is_some_and(|t| e.t.saturating_sub(t) <= 300);
                match out.last_mut() {
                    Some(Block::Key { vk: v, action: a @ KeyAction::Down }) if *v == vk && quick => *a = KeyAction::Press,
                    _ => push(&mut out, gap, Block::Key { vk, action: KeyAction::Up }),
                }
                last_t = e.t;
            }
        }
    }
    out
}

/// [Click, (short Wait), Click] on the same spot and button → one Double.
fn merge_double(out: &mut Vec<Block>) {
    let click = |b: &Block| match b {
        Block::Click { kind: ClickKind::Click, x, y, button, .. } => Some((x.clone(), y.clone(), *button)),
        _ => None,
    };
    let n = out.len();
    let Some(last) = click(&out[n - 1]) else { return };
    let short_wait = |b: &Block| matches!(b, Block::Wait { ms, .. } if ms.parse::<u32>().is_ok_and(|m| m <= 500));
    let first = if n >= 3 && short_wait(&out[n - 2]) {
        n - 3
    } else if n >= 2 {
        n - 2
    } else {
        return;
    };
    if click(&out[first]) == Some(last) {
        out.truncate(first + 1);
        if let Block::Click { kind, .. } = &mut out[first] {
            *kind = ClickKind::Double;
        }
    }
}

// ---------- real runs ----------

/// Focus runs share one physical cursor, so they take turns.
static FOCUS_LOCK: Mutex<()> = Mutex::new(());

pub struct RunHandle {
    pub script: String,
    pub window: String,
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<String>>,
    pub done: Arc<AtomicBool>,
    pub dbg: Arc<Debug>,
}

struct WinIo {
    inj: Injector,
    start: Instant,
    paused: u64,
    started_tick: u64,
    idle_ms: u64,
    ocr_scale: u32,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
}

impl WinIo {
    /// Focus mode: while the user is touching mouse/keyboard, release everything and wait
    /// until they've been idle for `idle_ms`. Paused time is excluded from the timeline.
    fn gate(&mut self) -> R {
        if self.inj.mode != Mode::Focus {
            return Ok(());
        }
        let active = |s: &Self| {
            let last = hooks::LAST_PHYSICAL.load(Ordering::Relaxed);
            last > s.started_tick && hooks::tick().saturating_sub(last) < s.idle_ms
        };
        if !active(self) {
            return Ok(());
        }
        let t0 = Instant::now();
        self.inj.release_all();
        *self.status.lock().unwrap() = "Pausado: você está usando o mouse/teclado".into();
        while active(self) {
            if self.stop.load(Ordering::Relaxed) {
                return Err(Halt::Stopped);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.paused += t0.elapsed().as_millis() as u64;
        *self.status.lock().unwrap() = "Rodando".into();
        self.inj.prepare();
        Ok(())
    }

    fn alive(&self) -> R {
        if win::is_alive(self.inj.top) { Ok(()) } else { Err("a janela-alvo foi fechada".into()) }
    }
}

impl Io for WinIo {
    fn send(&mut self, ev: &Ev) -> R {
        self.gate()?;
        self.alive()?;
        self.inj.send(ev);
        Ok(())
    }
    fn type_text(&mut self, s: &str) -> R {
        self.gate()?;
        self.alive()?;
        self.inj.type_text(s);
        Ok(())
    }
    fn pixel(&mut self, x: i32, y: i32) -> R<[u8; 3]> {
        let img = win::capture_client(self.inj.top, self.inj.mode)?;
        img.pixel(x, y).ok_or_else(|| Halt::Error(format!("pixel ({x},{y}) fora da janela")))
    }
    fn ocr(&mut self, r: Region) -> R<String> {
        let img = win::capture_client(self.inj.top, self.inj.mode)?;
        Ok(win::ocr(&img.crop(r).scale(self.ocr_scale))?)
    }
    fn window_exists(&mut self) -> bool {
        win::is_alive(self.inj.top)
    }
    fn window_foreground(&mut self) -> bool {
        win::is_foreground(self.inj.top)
    }
    fn window_title(&mut self) -> String {
        win::title_of(self.inj.top)
    }
    fn now_ms(&mut self) -> u64 {
        (self.start.elapsed().as_millis() as u64).saturating_sub(self.paused)
    }
    fn sleep(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
    fn pointer(&mut self, last_sent: Option<(i32, i32)>) -> (i32, i32) {
        match (self.inj.mode, last_sent) {
            (Mode::Focus, _) => win::cursor_client(self.inj.top),
            (Mode::Background, Some(p)) => p,
            (Mode::Background, None) => win::client_size(self.inj.top).map_or((0, 0), |(w, h)| (w / 2, h / 2)),
        }
    }
}

/// `delay_ms`: wait before starting (trigger delay), still stoppable.
pub fn spawn(script: Script, all: Vec<Script>, target: WinInfo, settings: &Settings, delay_ms: u64, log: Sender<String>, wake: Arc<dyn Fn() + Send + Sync>) -> RunHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new("Iniciando".to_string()));
    let done = Arc::new(AtomicBool::new(false));
    let dbg = Arc::new(Debug::default());
    let h = RunHandle {
        script: script.name.clone(),
        window: target.title.clone(),
        stop: stop.clone(),
        status: status.clone(),
        done: done.clone(),
        dbg: dbg.clone(),
    };
    let (idle_ms, ocr_scale) = ((settings.idle_resume_s * 1000.0) as u64, settings.ocr_scale);
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
        let focus = script.mode == Mode::Focus;
        let _turn = focus.then(|| {
            set("Na fila (modo Foco roda um por vez)");
            let g = FOCUS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            hooks::watch_physical(true);
            g
        });
        let result = if stop.load(Ordering::Relaxed) {
            Err(Halt::Stopped)
        } else {
            if let Some((w, hgt)) = script.client_size
                && let Err(e) = win::resize_client(target.hwnd, w, hgt)
            {
                say(format!("aviso: {e}"));
            }
            let mut io = WinIo {
                inj: Injector::new(target.hwnd, script.mode, script.simulate_focus),
                start: Instant::now(),
                paused: 0,
                started_tick: hooks::tick(),
                idle_ms,
                ocr_scale,
                stop: stop.clone(),
                status: status.clone(),
            };
            io.inj.prepare();
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
            io.inj.release_all();
            r
        };
        if focus {
            hooks::watch_physical(false);
        }
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

    #[derive(Default)]
    struct FakeIo {
        t: u64,
        sent: Vec<(u64, Ev)>,
        typed: Vec<String>,
        pixel: [u8; 3],
        /// Também dorme de verdade (para medir tempos reais, como o de parar).
        real_sleep: bool,
    }

    impl Io for FakeIo {
        fn send(&mut self, ev: &Ev) -> R {
            self.sent.push((self.t, *ev));
            Ok(())
        }
        fn type_text(&mut self, s: &str) -> R {
            self.typed.push(s.into());
            Ok(())
        }
        fn pixel(&mut self, _: i32, _: i32) -> R<[u8; 3]> {
            Ok(self.pixel)
        }
        fn ocr(&mut self, _: Region) -> R<String> {
            Ok("Pronto OK".into())
        }
        fn window_exists(&mut self) -> bool {
            true
        }
        fn window_foreground(&mut self) -> bool {
            false
        }
        fn window_title(&mut self) -> String {
            "Jogo - Nível 3".into()
        }
        fn now_ms(&mut self) -> u64 {
            self.t
        }
        fn sleep(&mut self, ms: u64) {
            self.t += ms;
            if self.real_sleep {
                std::thread::sleep(Duration::from_millis(ms));
            }
        }
    }

    fn run_with(blocks: Vec<Block>, others: Vec<Script>) -> (FakeIo, R) {
        let mut io = FakeIo::default();
        let stop = AtomicBool::new(false);
        let mut scripts = others;
        scripts.push(Script { name: "main".into(), blocks: blocks.clone(), ..Default::default() });
        let mut logs = |_m: String| {};
        let dbg = Debug::default();
        let r = Interp::new(&mut io, &stop, &scripts, 1.0, &mut logs, &dbg).run_blocks(&blocks);
        (io, r)
    }

    fn set(n: &str, e: &str) -> Block {
        Block::SetVar { name: n.into(), expr: e.into() }
    }
    fn ty(t: &str) -> Block {
        Block::Type { text: t.into() }
    }
    fn expr(e: &str) -> Condition {
        Condition { kind: CondKind::Expr(e.into()), negate: false }
    }

    #[test]
    fn if_vars_interpolation() {
        let (io, r) = run_with(vec![set("x", "5"), Block::If { cond: expr("x > 3"), then: vec![ty("{x * 2}")], else_: vec![ty("no")] }], vec![]);
        assert_eq!(r, Ok(()));
        assert_eq!(io.typed, ["10"]);
    }

    #[test]
    fn repeat_while_and_rand() {
        let b = vec![
            set("c", "0"),
            Block::Repeat { times: "3".into(), body: vec![set("c", "c + 1")] },
            Block::While { cond: expr("c < 7"), body: vec![set("c", "c + 1")] },
            ty("{c}"),
            Block::Repeat {
                times: "200".into(),
                body: vec![set("r", "rand(5, 7)"), Block::If { cond: expr("r < 5 || r > 7"), then: vec![ty("bad")], else_: vec![] }],
            },
            Block::If { cond: expr("contem(\"Olá Mundo\", \"mundo\")"), then: vec![ty("ok")], else_: vec![] },
        ];
        let (io, r) = run_with(b, vec![]);
        assert_eq!(r, Ok(()));
        assert_eq!(io.typed, ["7", "ok"]);
    }

    #[test]
    fn call_stop_and_recursion() {
        let sub = Script { name: "sub".into(), blocks: vec![ty("sub")], ..Default::default() };
        let (io, r) = run_with(vec![Block::Call { script: "sub".into() }, ty("a"), Block::Stop, ty("b")], vec![sub]);
        assert_eq!(r, Err(Halt::Stopped));
        assert_eq!(io.typed, ["sub", "a"]);

        let rec = Script { name: "rec".into(), blocks: vec![Block::Call { script: "rec".into() }], ..Default::default() };
        let (_, r) = run_with(vec![Block::Call { script: "rec".into() }], vec![rec]);
        assert!(matches!(r, Err(Halt::Error(e)) if e.contains("32")));
        let (_, r) = run_with(vec![Block::Call { script: "nope".into() }], vec![]);
        assert!(matches!(r, Err(Halt::Error(_))));
    }

    #[test]
    fn wait_until_timeout_and_conditions() {
        let red = Condition { kind: CondKind::Pixel { x: "1".into(), y: "1".into(), color: [255, 0, 0], tol: 10 }, negate: false };
        let (io, r) = run_with(vec![Block::WaitUntil { cond: red.clone(), timeout_ms: "1000".into(), on_timeout: OnTimeout::Stop }], vec![]);
        assert!(matches!(r, Err(Halt::Error(_))));
        assert!(io.t >= 1000);
        let (_, r) = run_with(vec![Block::WaitUntil { cond: red.clone(), timeout_ms: "300".into(), on_timeout: OnTimeout::Continue }, ty("after")], vec![]);
        assert_eq!(r, Ok(()));
        let b = vec![
            Block::If { cond: Condition { negate: true, ..red }, then: vec![ty("not red")], else_: vec![] },
            Block::If { cond: Condition { kind: CondKind::OcrContains { region: Region::default(), text: "pronto".into() }, negate: false }, then: vec![ty("ocr")], else_: vec![] },
            Block::If { cond: Condition { kind: CondKind::TitleMatches("jogo*".into()), negate: false }, then: vec![ty("title")], else_: vec![] },
            Block::OcrToVar { name: "t".into(), region: Region::default() },
            ty("{t}"),
        ];
        let (io, r) = run_with(b, vec![]);
        assert_eq!(r, Ok(()));
        assert_eq!(io.typed, ["not red", "ocr", "title", "Pronto OK"]);
    }

    #[test]
    fn recording_timing_and_clicks() {
        let events = vec![Event { t: 0, ev: Ev::Move(1, 1) }, Event { t: 100, ev: Ev::Move(2, 2) }, Event { t: 250, ev: Ev::Move(3, 3) }];
        let (io, _) = run_with(vec![Block::Recording { events: events.clone(), speed: 2.0 }], vec![]);
        assert_eq!(io.sent.iter().map(|s| s.0).collect::<Vec<_>>(), [0, 50, 125]);
        let (io, _) = run_with(vec![Block::Click { x: "1+1".into(), y: "3".into(), button: Btn::Left, kind: ClickKind::Double, jitter_px: 0 }], vec![]);
        let downs = io.sent.iter().filter(|s| matches!(s.1, Ev::Button(Btn::Left, true, 2, 3))).count();
        assert_eq!(downs, 2);
    }

    #[test]
    fn debug_log_trace_vars_and_breakpoint() {
        let mut io = FakeIo::default();
        let stop = AtomicBool::new(false);
        let dbg = Arc::new(Debug::default());
        let mut logs = vec![];
        let blocks = vec![set("a", "2"), set("nome", "\"Ana\""), Block::Log { text: "a={a * 10} {nome}".into() }, Block::Breakpoint, ty("depois")];
        // The UI's ▶ button: unpause shortly after the breakpoint hits.
        let d2 = dbg.clone();
        let unpause = std::thread::spawn(move || {
            while !d2.paused.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(30));
            d2.paused.store(false, Ordering::SeqCst);
        });
        {
            let mut log = |m: String| logs.push(m);
            let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut log, &dbg);
            it.trace = true;
            assert_eq!(it.run_blocks(&blocks), Ok(()));
        }
        unpause.join().unwrap();
        assert!(logs.contains(&"🐞 a=20 Ana".to_string()), "{logs:?}");
        assert!(logs.iter().any(|l| l == "» a = 2"));
        assert_eq!(*dbg.vars.lock().unwrap(), [("a".to_string(), "2".to_string()), ("nome".to_string(), "\"Ana\"".to_string())]);
        assert_eq!(*dbg.current.lock().unwrap(), "Digitar \"depois\"");
        assert_eq!(io.typed, ["depois"]);

        // Stop wins over a pause.
        dbg.paused.store(true, Ordering::SeqCst);
        stop.store(true, Ordering::SeqCst);
        let mut log = |_m: String| {};
        assert_eq!(Interp::new(&mut io, &stop, &[], 1.0, &mut log, &dbg).run_blocks(&[ty("x")]), Err(Halt::Stopped));
    }

    fn run_human(blocks: &[Block], human: bool) -> (FakeIo, R) {
        let mut io = FakeIo::default();
        let stop = AtomicBool::new(false);
        let mut logs = |_m: String| {};
        let dbg = Debug::default();
        let r = {
            let mut it = Interp::new(&mut io, &stop, &[], 1.0, &mut logs, &dbg);
            it.human_mouse = human;
            it.run_blocks(blocks)
        };
        (io, r)
    }

    #[test]
    fn human_mouse_glides_before_click() {
        let click = Block::Click { x: "600".into(), y: "400".into(), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 };
        let (io, r) = run_human(std::slice::from_ref(&click), true);
        assert_eq!(r, Ok(()));
        let down = io.sent.iter().position(|s| matches!(s.1, Ev::Button(Btn::Left, true, 600, 400))).unwrap();
        assert!(down >= 10, "só {down} movimentos antes do botão");
        assert!(io.sent[..down].iter().all(|s| matches!(s.1, Ev::Move(..))));
        assert_eq!(io.sent[down - 1].1, Ev::Move(600, 400), "o último movimento é o ponto do clique");
        // Os movimentos se espalham no tempo (não é um salto) e saem da posição inicial (0,0).
        assert!(io.sent[down - 1].0 >= 200, "trajeto durou {} ms", io.sent[down - 1].0);
        assert!(matches!(io.sent[0].1, Ev::Move(x, y) if x.abs() <= 30 && y.abs() <= 30));

        // Um arrasto (Down → Mover → Up) parte do ponto anterior, não da origem.
        let drag = [
            Block::Click { x: "100".into(), y: "100".into(), button: Btn::Left, kind: ClickKind::Down, jitter_px: 0 },
            Block::Move { x: "500".into(), y: "300".into() },
            Block::Click { x: "500".into(), y: "300".into(), button: Btn::Left, kind: ClickKind::Up, jitter_px: 0 },
        ];
        let (io, r) = run_human(&drag, true);
        assert_eq!(r, Ok(()));
        let d = io.sent.iter().position(|s| matches!(s.1, Ev::Button(_, true, ..))).unwrap();
        let u = io.sent.iter().position(|s| matches!(s.1, Ev::Button(_, false, ..))).unwrap();
        assert!(u - d > 10, "arrasto com só {} movimentos", u - d - 1);
        assert!(matches!(io.sent[d + 1].1, Ev::Move(x, y) if (x - 100).abs() <= 20 && (y - 100).abs() <= 20), "{:?}", io.sent[d + 1]);
        assert_eq!(io.sent[u - 1].1, Ev::Move(500, 300));

        // Rolar também desliza antes da roda.
        let (io, _) = run_human(&[Block::Scroll { x: "300".into(), y: "200".into(), delta: -120 }], true);
        assert!(io.sent.len() > 5);
        assert_eq!(io.sent.last().unwrap().1, Ev::Wheel(-120, 300, 200));
    }

    #[test]
    fn human_mouse_off_keeps_old_sequence() {
        let blocks = [
            Block::Move { x: "5".into(), y: "6".into() },
            Block::Click { x: "600".into(), y: "400".into(), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 },
            Block::Scroll { x: "300".into(), y: "200".into(), delta: -120 },
        ];
        let (io, r) = run_human(&blocks, false);
        assert_eq!(r, Ok(()));
        let b = |d| Ev::Button(Btn::Left, d, 600, 400);
        assert_eq!(io.sent, [(0, Ev::Move(5, 6)), (0, Ev::Move(600, 400)), (0, b(true)), (30, b(false)), (60, Ev::Wheel(-120, 300, 200))]);
    }

    #[test]
    fn stop_interrupts_human_path_fast() {
        // Velocidade 0,05: um trajeto de ~3000 px leva vários segundos.
        let mut io = FakeIo { real_sleep: true, ..Default::default() };
        let stop = Arc::new(AtomicBool::new(false));
        let asked = Arc::new(Mutex::new(None::<Instant>));
        let (s2, a2) = (stop.clone(), asked.clone());
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            *a2.lock().unwrap() = Some(Instant::now());
            s2.store(true, Ordering::SeqCst);
        });
        let mut logs = |_m: String| {};
        let dbg = Debug::default();
        let click = Block::Click { x: "3000".into(), y: "1500".into(), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 };
        let r = {
            let mut it = Interp::new(&mut io, &stop, &[], 0.05, &mut logs, &dbg);
            it.human_mouse = true;
            it.run_blocks(&[click])
        };
        let returned = Instant::now();
        stopper.join().unwrap();
        assert_eq!(r, Err(Halt::Stopped));
        let took = returned - asked.lock().unwrap().unwrap();
        assert!(took <= Duration::from_millis(50), "parou em {took:?}");
        assert!(!io.sent.is_empty(), "parou antes de começar o trajeto");
        assert!(io.sent.iter().all(|s| matches!(s.1, Ev::Move(..))), "chegou a clicar");
    }

    #[test]
    fn bad_expression_is_an_error() {
        let (_, r) = run_with(vec![Block::Wait { ms: "nope +".into(), jitter_ms: 0 }], vec![]);
        assert!(matches!(r, Err(Halt::Error(_))));
    }

    #[test]
    fn explode_recording() {
        let e = |t, ev| Event { t, ev };
        let ev = vec![
            e(0, Ev::Move(5, 5)),
            e(100, Ev::Button(Btn::Left, true, 10, 10)),
            e(150, Ev::Button(Btn::Left, false, 10, 10)),
            e(250, Ev::Button(Btn::Left, true, 10, 10)),
            e(300, Ev::Button(Btn::Left, false, 10, 10)),
            e(1000, Ev::Key(0x41, 30, false, true)),
            e(1050, Ev::Key(0x41, 30, false, false)),
            e(2000, Ev::Key(0x57, 17, false, true)),
            e(2100, Ev::Key(0x57, 17, false, true)), // autorepeat
            e(4000, Ev::Key(0x57, 17, false, false)),
            e(5000, Ev::Button(Btn::Left, true, 0, 0)),
            e(5100, Ev::Move(50, 50)),
            e(5200, Ev::Button(Btn::Left, false, 90, 90)),
        ];
        let out = explode(&ev);
        let kinds: Vec<String> = out
            .iter()
            .map(|b| match b {
                Block::Click { kind, .. } => format!("{kind:?}"),
                Block::Key { action, .. } => format!("{action:?}"),
                Block::Wait { .. } => "W".into(),
                Block::Move { .. } => "M".into(),
                o => o.title().into(),
            })
            .collect();
        assert_eq!(kinds, ["W", "Double", "W", "Press", "W", "Down", "W", "Up", "W", "Down", "W", "M", "W", "Up"]);
    }
}
