use crate::uia::{UiaAction, UiaElem, UiaTest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Mode {
    #[default]
    Background,
    Focus,
}

/// Identifies the target window across restarts. `title` accepts `*` and `?`.
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TargetRule {
    pub exe: String,
    pub title: String,
}

impl TargetRule {
    pub fn matches(&self, exe: &str, title: &str) -> bool {
        exe.eq_ignore_ascii_case(&self.exe) && glob_match(&self.title, title)
    }
}

/// When a script starts by itself (besides the play button / hotkeys).
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
pub enum Trigger {
    #[default]
    Manual,
    /// Once per UltraTask session, as soon as a matching window exists.
    OnStartup { delay_s: f32 },
    /// Every time a new matching window appears (windows already open when UltraTask starts don't count).
    OnWindowOpen { delay_s: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u16, // 0 = unset
}

impl Hotkey {
    pub const fn csa(vk: u16) -> Self {
        Hotkey { ctrl: true, shift: true, alt: true, win: false, vk }
    }
    pub fn label(&self) -> String {
        if self.vk == 0 {
            return "—".into();
        }
        let mut s = String::new();
        for (on, n) in [(self.ctrl, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+"), (self.win, "Win+")] {
            if on {
                s += n;
            }
        }
        s + &key_name(self.vk)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Btn {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Ev {
    Move(i32, i32),
    Button(Btn, bool, i32, i32), // down?, client x, y
    Wheel(i32, i32, i32),        // delta, client x, y
    Key(u16, u16, bool, bool),   // vk, scan, extended?, down?
}

/// `t` = ms since recording start; coordinates are client-area physical pixels.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Event {
    pub t: u32,
    pub ev: Ev,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ClickKind {
    Click,
    Double,
    Down,
    Up,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Down,
    Up,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum OnTimeout {
    Stop,
    Continue,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum CondKind {
    Pixel { x: String, y: String, color: [u8; 3], tol: u8 },
    OcrContains { region: Region, text: String },
    WindowExists,
    WindowForeground,
    TitleMatches(String),
    Expr(String),
    /// UI Automation element of the target window.
    Uia {
        #[serde(default)]
        elem: UiaElem,
        #[serde(default)]
        test: UiaTest,
    },
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub kind: CondKind,
    pub negate: bool,
}

impl Default for Condition {
    fn default() -> Self {
        Condition { kind: CondKind::WindowExists, negate: false }
    }
}

/// Numeric fields hold evalexpr expressions ("120", "x0 + 10").
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Block {
    Recording {
        events: Vec<Event>,
        speed: f32,
        /// UIA element of each recorded click (indices into `events`); used by "Explodir".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        elements: Vec<crate::uia_rec::RecElem>,
    },
    Move { x: String, y: String },
    Click { x: String, y: String, button: Btn, kind: ClickKind, jitter_px: u32 },
    Key { vk: u16, action: KeyAction },
    Type { text: String },
    Scroll { x: String, y: String, delta: i32 },
    Wait { ms: String, jitter_ms: u32 },
    WaitUntil { cond: Condition, timeout_ms: String, on_timeout: OnTimeout },
    If { cond: Condition, then: Vec<Block>, else_: Vec<Block> },
    Repeat { times: String, body: Vec<Block> },
    While { cond: Condition, body: Vec<Block> },
    SetVar { name: String, expr: String },
    OcrToVar { name: String, region: Region },
    /// Acts on a control through UI Automation: no mouse or keyboard input at all.
    Uia {
        #[serde(default)]
        elem: UiaElem,
        #[serde(default)]
        action: UiaAction,
        /// expr: how long to wait for the element to appear.
        #[serde(default = "crate::uia::default_timeout")]
        timeout_ms: String,
        /// Element not found: click the captured point instead of stopping with an error.
        #[serde(default)]
        fallback_click: bool,
    },
    Call { script: String },
    Log { text: String },
    Breakpoint,
    Stop,
}

impl Block {
    pub fn title(&self) -> &'static str {
        match self {
            Block::Recording { .. } => "Reproduzir gravação",
            Block::Move { .. } => "Mover",
            Block::Click { .. } => "Clique",
            Block::Key { .. } => "Tecla",
            Block::Type { .. } => "Digitar",
            Block::Scroll { .. } => "Rolar",
            Block::Wait { .. } => "Esperar",
            Block::WaitUntil { .. } => "Esperar até",
            Block::If { .. } => "Se",
            Block::Repeat { .. } => "Repetir",
            Block::While { .. } => "Enquanto",
            Block::SetVar { .. } => "Definir variável",
            Block::OcrToVar { .. } => "Ler texto (OCR)",
            Block::Uia { .. } => "Elemento (UIA)",
            Block::Call { .. } => "Chamar script",
            Block::Log { .. } => "Mostrar no log",
            Block::Breakpoint => "Breakpoint (pausar)",
            Block::Stop => "Parar",
        }
    }

    /// One line describing what the block does, for the debug panel and trace log.
    pub fn summary(&self) -> String {
        match self {
            Block::Recording { events, speed, .. } => format!("Reproduzir gravação ({} eventos, {speed}×)", events.len()),
            Block::Move { x, y } => format!("Mover para ({x}, {y})"),
            Block::Click { x, y, button, kind, .. } => format!("Clique {kind:?} {button:?} em ({x}, {y})"),
            Block::Key { vk, action } => format!("Tecla {} {action:?}", key_name(*vk)),
            Block::Type { text } => format!("Digitar \"{text}\""),
            Block::Scroll { x, y, delta } => format!("Rolar {delta} em ({x}, {y})"),
            Block::Wait { ms, jitter_ms } => format!("Esperar {ms} ms ±{jitter_ms}"),
            Block::WaitUntil { timeout_ms, .. } => format!("Esperar até condição (timeout {timeout_ms} ms)"),
            Block::If { .. } => "Se …".into(),
            Block::Repeat { times, .. } => format!("Repetir {times} vezes"),
            Block::While { .. } => "Enquanto …".into(),
            Block::SetVar { name, expr } => format!("{name} = {expr}"),
            Block::OcrToVar { name, region } => format!("{name} = OCR ({}, {}, {}×{})", region.x, region.y, region.w, region.h),
            Block::Uia { elem, action, .. } => format!("Elemento {}: {}", crate::uia::describe(elem), action.label()),
            Block::Call { script } => format!("Chamar '{script}'"),
            Block::Log { text } => format!("Log \"{text}\""),
            Block::Breakpoint => "Breakpoint".into(),
            Block::Stop => "Parar".into(),
        }
    }

    /// One fresh instance of every block type, for the block palette.
    pub fn templates() -> Vec<Block> {
        let s = |v: &str| v.to_string();
        vec![
            Block::Click { x: s("0"), y: s("0"), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 },
            Block::Move { x: s("0"), y: s("0") },
            Block::Key { vk: 0x0D, action: KeyAction::Press },
            Block::Type { text: String::new() },
            Block::Scroll { x: s("0"), y: s("0"), delta: -120 },
            Block::Uia { elem: UiaElem::default(), action: UiaAction::Invoke, timeout_ms: crate::uia::default_timeout(), fallback_click: false },
            Block::Wait { ms: s("500"), jitter_ms: 0 },
            Block::WaitUntil { cond: Condition::default(), timeout_ms: s("10000"), on_timeout: OnTimeout::Stop },
            Block::If { cond: Condition::default(), then: vec![], else_: vec![] },
            Block::Repeat { times: s("3"), body: vec![] },
            Block::While { cond: Condition::default(), body: vec![] },
            Block::SetVar { name: s("contador"), expr: s("0") },
            Block::OcrToVar { name: s("texto"), region: Region { x: 0, y: 0, w: 200, h: 40 } },
            Block::Call { script: String::new() },
            Block::Log { text: s("contador = {contador}") },
            Block::Breakpoint,
            Block::Stop,
        ]
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Script {
    pub name: String,
    pub target: Option<TargetRule>,
    pub mode: Mode,
    pub simulate_focus: bool,
    pub speed: f32,
    pub repeat: u32, // 0 = forever
    pub hotkey: Option<Hotkey>,
    pub client_size: Option<(i32, i32)>,
    pub trigger: Trigger,
    /// Debug: log every block as it runs.
    pub trace: bool,
    /// Mover/Clique/Rolar levam o ponteiro por um trajeto humano em vez de teletransportar.
    pub human_mouse: bool,
    pub blocks: Vec<Block>,
    /// Aba Integrações: rodam em paralelo ao fluxo (veja `integrations`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub integrations: Vec<crate::integrations::Integration>,
}

impl Default for Script {
    fn default() -> Self {
        Script {
            name: String::new(),
            target: None,
            mode: Mode::Background,
            simulate_focus: false,
            speed: 1.0,
            repeat: 1,
            hotkey: None,
            client_size: None,
            trigger: Trigger::Manual,
            trace: false,
            human_mouse: false,
            blocks: vec![],
            integrations: vec![],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub record: Hotkey,
    pub play: Hotkey,
    pub panic: Hotkey,
    pub idle_resume_s: f32,
    pub ocr_scale: u32,
    pub theme: Theme,
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            record: Hotkey::csa(b'R' as u16),
            play: Hotkey::csa(b'P' as u16),
            panic: Hotkey::csa(b'X' as u16),
            idle_resume_s: 2.0,
            ocr_scale: 2,
            theme: Theme::System,
            start_with_windows: false,
        }
    }
}

// ---------- storage (portable: next to the exe) ----------

pub fn base_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn scripts_dir() -> PathBuf {
    base_dir().join("scripts")
}

pub fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    format!("{}.json", clean.trim())
}

/// Write to a temp file and rename over the target so a crash never leaves a half-written script.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Returns scripts sorted by name plus human-readable errors for files that failed to parse.
pub fn load_scripts() -> (Vec<Script>, Vec<String>) {
    let (mut out, mut errs) = (vec![], vec![]);
    let Ok(dir) = std::fs::read_dir(scripts_dir()) else { return (out, errs) };
    for entry in dir.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "json") {
            match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<Script>(&b).map_err(|e| e.to_string())) {
                Ok(s) => out.push(s),
                Err(e) => errs.push(format!("{}: {e}", p.display())),
            }
        }
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    (out, errs)
}

pub fn save_script(s: &Script) -> std::io::Result<()> {
    write_atomic(&scripts_dir().join(file_name(&s.name)), &serde_json::to_vec(s)?)
}

pub fn delete_script(name: &str) -> std::io::Result<()> {
    std::fs::remove_file(scripts_dir().join(file_name(name)))
}

pub fn load_settings() -> Settings {
    std::fs::read(base_dir().join("ultratask.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> std::io::Result<()> {
    write_atomic(&base_dir().join("ultratask.json"), &serde_json::to_vec_pretty(s)?)
}

// ---------- small helpers ----------

/// Case-insensitive glob with `*` and `?`.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Replaces `{expr}` with `eval(expr)`; `{{` and `}}` are literal braces.
pub fn interpolate(text: &str, mut eval: impl FnMut(&str) -> Result<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let expr: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out += &eval(&expr)?;
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

// ---------- validation (editor highlighting + `ultratask --validate`) ----------

pub fn expr_error(s: &str) -> Option<String> {
    // evalexpr only notices "x >" (missing operand) when evaluating, so check arity here.
    fn arity_ok(n: &evalexpr::Node) -> bool {
        use evalexpr::Operator::*;
        let want = match n.operator() {
            Add | Sub | Mul | Div | Mod | Exp | Eq | Neq | Gt | Lt | Geq | Leq | And | Or | Assign | AddAssign | SubAssign | MulAssign | DivAssign | ModAssign | ExpAssign | AndAssign | OrAssign => Some(2),
            Not | Neg => Some(1),
            _ => None,
        };
        want.is_none_or(|w| n.children().len() == w) && n.children().iter().all(arity_ok)
    }
    if s.trim().is_empty() {
        return Some("vazia".into());
    }
    match evalexpr::build_operator_tree::<evalexpr::DefaultNumericTypes>(s) {
        Err(e) => Some(e.to_string()),
        Ok(n) if !arity_ok(&n) => Some("operador sem operando".into()),
        Ok(_) => None,
    }
}

pub fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_alphabetic() || f == '_') && c.all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// Problems that would make the script fail at run time, each prefixed with its JSON path.
pub fn validate(s: &Script, known: &[String]) -> Vec<String> {
    let mut errs = vec![];
    if s.name.trim().is_empty() {
        errs.push("name: vazio".into());
    }
    if !(s.speed > 0.0) {
        errs.push(format!("speed: deve ser > 0 (é {})", s.speed));
    }
    if s.target.as_ref().is_some_and(|t| t.exe.trim().is_empty()) {
        errs.push("target.exe: vazio".into());
    }
    if s.trigger != Trigger::Manual && s.target.is_none() {
        errs.push("trigger: gatilhos precisam de um target".into());
    }
    validate_blocks(&s.blocks, "blocks", known, &mut errs);
    crate::integrations::validate(&s.integrations, &mut errs);
    errs
}

fn check_expr(errs: &mut Vec<String>, path: &str, s: &str) {
    if let Some(e) = expr_error(s) {
        errs.push(format!("{path}: expressão inválida \"{s}\": {e}"));
    }
}

fn check_cond(errs: &mut Vec<String>, path: &str, c: &Condition) {
    match &c.kind {
        CondKind::Pixel { x, y, .. } => {
            check_expr(errs, &format!("{path}.kind.Pixel.x"), x);
            check_expr(errs, &format!("{path}.kind.Pixel.y"), y);
        }
        CondKind::OcrContains { region, .. } if region.w <= 0 || region.h <= 0 => errs.push(format!("{path}.kind.OcrContains.region: w e h devem ser > 0")),
        CondKind::Expr(e) => check_expr(errs, &format!("{path}.kind.Expr"), e),
        CondKind::Uia { elem, .. } => check_uia_elem(errs, &format!("{path}.kind.Uia.elem"), elem),
        _ => {}
    }
}

fn check_uia_elem(errs: &mut Vec<String>, path: &str, e: &UiaElem) {
    for (field, m) in crate::uia::elem_errors(e) {
        errs.push(if field.is_empty() { format!("{path}: {m}") } else { format!("{path}.{field}: {m}") });
    }
}

fn validate_blocks(blocks: &[Block], path: &str, known: &[String], errs: &mut Vec<String>) {
    for (i, b) in blocks.iter().enumerate() {
        let p = format!("{path}[{i}]");
        let f = |field: &str| format!("{p}.{field}");
        match b {
            Block::Recording { speed, .. } if !(*speed > 0.0) => errs.push(format!("{}: deve ser > 0", f("speed"))),
            Block::Recording { events, elements, .. } => {
                for (j, r) in elements.iter().enumerate() {
                    if !events.get(r.event as usize).is_some_and(|e| matches!(e.ev, Ev::Button(_, true, _, _))) {
                        errs.push(format!("{}: o evento {} não é um botão pressionado", f(&format!("elements[{j}].event")), r.event));
                    }
                }
            }
            Block::Move { x, y } | Block::Click { x, y, .. } | Block::Scroll { x, y, .. } => {
                check_expr(errs, &f("x"), x);
                check_expr(errs, &f("y"), y);
            }
            Block::Key { vk: 0, .. } => errs.push(format!("{}: 0 não é uma tecla", f("vk"))),
            Block::Type { text } | Block::Log { text } => {
                if let Err(e) = interpolate(text, |e| expr_error(e).map_or(Ok(String::new()), |m| Err(format!("{{{e}}}: {m}")))) {
                    errs.push(format!("{}: {e}", f("text")));
                }
            }
            Block::Wait { ms, .. } => check_expr(errs, &f("ms"), ms),
            Block::WaitUntil { cond, timeout_ms, .. } => {
                check_cond(errs, &f("cond"), cond);
                check_expr(errs, &f("timeout_ms"), timeout_ms);
            }
            Block::If { cond, then, else_ } => {
                check_cond(errs, &f("cond"), cond);
                validate_blocks(then, &f("then"), known, errs);
                validate_blocks(else_, &f("else_"), known, errs);
            }
            Block::Repeat { times, body } => {
                check_expr(errs, &f("times"), times);
                validate_blocks(body, &f("body"), known, errs);
            }
            Block::While { cond, body } => {
                check_cond(errs, &f("cond"), cond);
                validate_blocks(body, &f("body"), known, errs);
            }
            Block::SetVar { name, expr } => {
                if !is_ident(name) {
                    errs.push(format!("{}: \"{name}\" não é um nome de variável válido", f("name")));
                }
                check_expr(errs, &f("expr"), expr);
            }
            Block::OcrToVar { name, region } => {
                if !is_ident(name) {
                    errs.push(format!("{}: \"{name}\" não é um nome de variável válido", f("name")));
                }
                if region.w <= 0 || region.h <= 0 {
                    errs.push(format!("{}: w e h devem ser > 0", f("region")));
                }
            }
            Block::Uia { elem, action, timeout_ms, .. } => {
                check_uia_elem(errs, &f("elem"), elem);
                check_expr(errs, &f("timeout_ms"), timeout_ms);
                match action {
                    UiaAction::ReadValue(v) | UiaAction::ReadName(v) if !is_ident(v) => errs.push(format!("{}: \"{v}\" não é um nome de variável válido", f("action"))),
                    UiaAction::SetValue(t) => {
                        if let Err(e) = interpolate(t, |e| expr_error(e).map_or(Ok(String::new()), |m| Err(format!("{{{e}}}: {m}")))) {
                            errs.push(format!("{}: {e}", f("action.SetValue")));
                        }
                    }
                    _ => {}
                }
            }
            Block::Call { script } if !known.contains(script) => errs.push(format!("{}: script \"{script}\" não encontrado na pasta", f("script"))),
            _ => {}
        }
    }
}

/// `ultratask --validate a.json b.json`: prints OK/ERRO lines, returns the process exit code.
pub fn validate_files(paths: &[String]) -> i32 {
    if paths.is_empty() {
        println!("uso: ultratask --validate <script.json>...");
        return 2;
    }
    let mut failed = false;
    for p in paths {
        let path = std::path::Path::new(p);
        let parsed = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Script>(&b).map_err(|e| format!("JSON inválido: {e}")));
        let s = match parsed {
            Ok(s) => s,
            Err(e) => {
                println!("ERRO {p}: {e}");
                failed = true;
                continue;
            }
        };
        // "Chamar script" may point at any script in the same folder.
        let mut known = vec![s.name.clone()];
        if let Some(dir) = path.parent().and_then(|d| std::fs::read_dir(if d.as_os_str().is_empty() { std::path::Path::new(".") } else { d }).ok()) {
            for e in dir.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")) {
                if let Ok(o) = std::fs::read(e.path()).map_err(|_| ()).and_then(|b| serde_json::from_slice::<Script>(&b).map_err(|_| ())) {
                    known.push(o.name);
                }
            }
        }
        let mut errs = validate(&s, &known);
        let want = file_name(&s.name);
        if path.file_name().and_then(|f| f.to_str()) != Some(want.as_str()) {
            errs.push(format!("o arquivo deveria se chamar \"{want}\" (derivado de name)"));
        }
        if errs.is_empty() {
            println!("OK {p}");
        } else {
            failed = true;
            for e in errs {
                println!("ERRO {p}: {e}");
            }
        }
    }
    i32::from(failed)
}

pub const KEYS: &[(u16, &str)] = &[
    (0x08, "Backspace"), (0x09, "Tab"), (0x0D, "Enter"), (0x10, "Shift"), (0x11, "Ctrl"), (0x12, "Alt"),
    (0x13, "Pause"), (0x14, "CapsLock"), (0x1B, "Esc"), (0x20, "Espaço"), (0x21, "PgUp"), (0x22, "PgDn"),
    (0x23, "End"), (0x24, "Home"), (0x25, "⬅"), (0x26, "⬆"), (0x27, "➡"), (0x28, "⬇"), (0x2C, "PrintScreen"),
    (0x2D, "Insert"), (0x2E, "Delete"), (0x5B, "Win"), (0x60, "Num0"), (0x61, "Num1"), (0x62, "Num2"),
    (0x63, "Num3"), (0x64, "Num4"), (0x65, "Num5"), (0x66, "Num6"), (0x67, "Num7"), (0x68, "Num8"),
    (0x69, "Num9"), (0x6A, "Num*"), (0x6B, "Num+"), (0x6D, "Num-"), (0x6E, "Num."), (0x6F, "Num/"),
    (0xA0, "LShift"), (0xA1, "RShift"), (0xA2, "LCtrl"), (0xA3, "RCtrl"), (0xA4, "LAlt"), (0xA5, "RAlt"),
];

pub fn key_name(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => KEYS.iter().find(|k| k.0 == vk).map(|k| k.1.to_string()).unwrap_or_else(|| format!("VK {vk:#04X}")),
    }
}

/// Every key the editor offers, in display order.
pub fn all_keys() -> Vec<u16> {
    (0x41..=0x5A).chain(0x30..=0x39).chain(0x70..=0x7B).chain(KEYS.iter().map(|k| k.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob() {
        assert!(glob_match("*bloco de notas*", "Sem título - Bloco de Notas"));
        assert!(glob_match("jogo?.exe", "JOGO2.exe"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("a*b", "acd"));
        assert!(glob_match("a*b*c", "axxbyyc"));
    }

    #[test]
    fn interp() {
        let r = interpolate("x={a} {{lit}} {b}", |e| Ok(e.to_uppercase())).unwrap();
        assert_eq!(r, "x=A {lit} B");
        assert!(interpolate("{bad}", |_| Err("e".into())).is_err());
    }

    #[test]
    fn json_roundtrip() {
        let s = Script {
            name: "t".into(),
            blocks: vec![
                Block::Recording { events: vec![Event { t: 5, ev: Ev::Key(65, 30, false, true) }], speed: 1.0, elements: vec![] },
                Block::If { cond: Condition::default(), then: Block::templates(), else_: vec![Block::Stop] },
            ],
            ..Default::default()
        };
        let back: Script = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn validation() {
        let s = |v: &str| v.to_string();
        let ok = Script {
            name: s("ok"),
            target: Some(TargetRule { exe: s("notepad.exe"), title: s("*") }),
            trigger: Trigger::OnWindowOpen { delay_s: 1.0 },
            blocks: vec![
                Block::SetVar { name: s("n"), expr: s("rand(1, 3) * 2") },
                Block::Log { text: s("n = {n}") },
                Block::Repeat { times: s("n"), body: vec![Block::Call { script: s("sub") }] },
            ],
            ..Default::default()
        };
        assert_eq!(validate(&ok, &[s("sub")]), Vec::<String>::new());

        let bad = Script {
            name: s("bad"),
            trigger: Trigger::OnStartup { delay_s: 0.0 },
            blocks: vec![Block::If {
                cond: Condition { kind: CondKind::Expr(s("x >")), negate: false },
                then: vec![Block::SetVar { name: s("1x"), expr: s("2") }, Block::Key { vk: 0, action: KeyAction::Press }],
                else_: vec![Block::Type { text: s("oi {a +}") }, Block::Call { script: s("nope") }],
            }],
            ..Default::default()
        };
        let errs = validate(&bad, &[]);
        let paths: Vec<&str> = errs.iter().map(|e| e.split(':').next().unwrap()).collect();
        assert_eq!(paths, ["trigger", "blocks[0].cond.kind.Expr", "blocks[0].then[0].name", "blocks[0].then[1].vk", "blocks[0].else_[0].text", "blocks[0].else_[1].script"], "{errs:#?}");
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(serde_json::from_str::<Script>(r#"{"name":"x","trgger":"Manual"}"#).is_err());
        assert!(serde_json::from_str::<Script>(r#"{"name":"x","blocks":[{"Wait":{"ms":"1","jitter":0}}]}"#).is_err());
        let s: Script = serde_json::from_str(r#"{"name":"x","blocks":["Stop",{"Wait":{"ms":"1","jitter_ms":0}}]}"#).unwrap();
        assert_eq!(s.blocks.len(), 2);
        assert_eq!(s.speed, 1.0); // missing fields take defaults
    }

    /// Ticket 17: a recording saved before `elements` existed parses, validates and its
    /// block is written back byte for byte.
    #[test]
    fn old_recordings_unchanged() {
        let old = r#"{"name":"velho","target":null,"mode":"Background","simulate_focus":false,"speed":1.0,"repeat":1,"hotkey":null,"client_size":[800,600],"trigger":"Manual","trace":false,"blocks":[{"Recording":{"events":[{"t":0,"ev":{"Move":[10,20]}},{"t":90,"ev":{"Button":["Left",true,10,20]}},{"t":150,"ev":{"Button":["Left",false,10,20]}}],"speed":1.0}}]}"#;
        let s: Script = serde_json::from_str(old).unwrap();
        assert!(matches!(&s.blocks[0], Block::Recording { elements, .. } if elements.is_empty()));
        assert_eq!(validate(&s, &[]), Vec::<String>::new());
        let block = &old[old.find(r#"{"Recording""#).unwrap()..old.len() - 2];
        assert_eq!(serde_json::to_string(&s.blocks[0]).unwrap(), block);
        let new = old.replace(r#""speed":1.0}}]"#, r#""speed":1.0,"elements":[{"event":1,"elem":{"name":"OK","control_type":"Button","x":10,"y":20},"action":"Invoke"}]}}]"#);
        let s: Script = serde_json::from_str(&new).unwrap();
        assert!(matches!(&s.blocks[0], Block::Recording { elements, .. } if elements.len() == 1));
        assert_eq!(validate(&s, &[]), Vec::<String>::new());
        let s: Script = serde_json::from_str(&new.replace(r#""event":1"#, r#""event":0"#)).unwrap();
        assert_eq!(validate(&s, &[]), ["blocks[0].elements[0].event: o evento 0 não é um botão pressionado"]);
    }

    #[test]
    fn uia_validation() {
        use crate::uia::UiaElem;
        let s = |v: &str| v.to_string();
        let ok_elem = UiaElem { automation_id: s("btnOk"), control_type: s("Button"), ..Default::default() };
        // Only elem and action are needed; timeout and fallback take defaults.
        let json = r#"{"name":"u","blocks":[{"Uia":{"elem":{"automation_id":"btnOk","control_type":"Button"},"action":"Invoke"}},
            {"If":{"cond":{"kind":{"Uia":{"elem":{"name":"OK"}}},"negate":false},"then":[],"else_":[]}}]}"#;
        let parsed: Script = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.blocks[0], Block::Uia { elem: ok_elem, action: UiaAction::Invoke, timeout_ms: s("5000"), fallback_click: false });
        assert!(matches!(&parsed.blocks[1], Block::If { cond: Condition { kind: CondKind::Uia { test: UiaTest::Exists, .. }, .. }, .. }));
        assert_eq!(validate(&parsed, &[]), Vec::<String>::new());
        let bad = Script {
            name: s("bad"),
            blocks: vec![
                Block::Uia { elem: UiaElem::default(), action: UiaAction::ReadValue(s("1x")), timeout_ms: s("x >"), fallback_click: true },
                Block::Uia { elem: UiaElem { control_type: s("Botao"), ..Default::default() }, action: UiaAction::SetValue(s("{a +}")), timeout_ms: s("100"), fallback_click: false },
                Block::WaitUntil { cond: Condition { kind: CondKind::Uia { elem: UiaElem::default(), test: UiaTest::Checked }, negate: false }, timeout_ms: s("1"), on_timeout: OnTimeout::Stop },
            ],
            ..Default::default()
        };
        let errs = validate(&bad, &[]);
        let paths: Vec<&str> = errs.iter().map(|e| e.split(':').next().unwrap()).collect();
        assert_eq!(paths, ["blocks[0].elem", "blocks[0].timeout_ms", "blocks[0].action", "blocks[1].elem.control_type", "blocks[1].action.SetValue", "blocks[2].cond.kind.Uia.elem"], "{errs:#?}");
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("a/b:c"), "a_b_c.json");
    }
}
