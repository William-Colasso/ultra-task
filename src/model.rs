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
pub struct TargetRule {
    pub exe: String,
    pub title: String,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
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
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum CondKind {
    Pixel { x: String, y: String, color: [u8; 3], tol: u8 },
    OcrContains { region: Region, text: String },
    WindowExists,
    WindowForeground,
    TitleMatches(String),
    Expr(String),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
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
pub enum Block {
    Recording { events: Vec<Event>, speed: f32 },
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
    Call { script: String },
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
            Block::Call { .. } => "Chamar script",
            Block::Stop => "Parar",
        }
    }

    /// One fresh instance of every block type, for the "+ bloco" menu.
    pub fn templates() -> Vec<Block> {
        let s = |v: &str| v.to_string();
        vec![
            Block::Click { x: s("0"), y: s("0"), button: Btn::Left, kind: ClickKind::Click, jitter_px: 0 },
            Block::Move { x: s("0"), y: s("0") },
            Block::Key { vk: 0x0D, action: KeyAction::Press },
            Block::Type { text: String::new() },
            Block::Scroll { x: s("0"), y: s("0"), delta: -120 },
            Block::Wait { ms: s("500"), jitter_ms: 0 },
            Block::WaitUntil { cond: Condition::default(), timeout_ms: s("10000"), on_timeout: OnTimeout::Stop },
            Block::If { cond: Condition::default(), then: vec![], else_: vec![] },
            Block::Repeat { times: s("3"), body: vec![] },
            Block::While { cond: Condition::default(), body: vec![] },
            Block::SetVar { name: s("contador"), expr: s("0") },
            Block::OcrToVar { name: s("texto"), region: Region { x: 0, y: 0, w: 200, h: 40 } },
            Block::Call { script: String::new() },
            Block::Stop,
        ]
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Script {
    pub name: String,
    pub target: Option<TargetRule>,
    pub mode: Mode,
    pub simulate_focus: bool,
    pub speed: f32,
    pub repeat: u32, // 0 = forever
    pub hotkey: Option<Hotkey>,
    pub client_size: Option<(i32, i32)>,
    pub blocks: Vec<Block>,
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
            blocks: vec![],
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            record: Hotkey::csa(b'R' as u16),
            play: Hotkey::csa(b'P' as u16),
            panic: Hotkey::csa(b'X' as u16),
            idle_resume_s: 2.0,
            ocr_scale: 2,
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

pub const KEYS: &[(u16, &str)] = &[
    (0x08, "Backspace"), (0x09, "Tab"), (0x0D, "Enter"), (0x10, "Shift"), (0x11, "Ctrl"), (0x12, "Alt"),
    (0x13, "Pause"), (0x14, "CapsLock"), (0x1B, "Esc"), (0x20, "Espaço"), (0x21, "PgUp"), (0x22, "PgDn"),
    (0x23, "End"), (0x24, "Home"), (0x25, "←"), (0x26, "↑"), (0x27, "→"), (0x28, "↓"), (0x2C, "PrintScreen"),
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
                Block::Recording { events: vec![Event { t: 5, ev: Ev::Key(65, 30, false, true) }], speed: 1.0 },
                Block::If { cond: Condition::default(), then: Block::templates(), else_: vec![Block::Stop] },
            ],
            ..Default::default()
        };
        let back: Script = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("a/b:c"), "a_b_c.json");
    }
}
