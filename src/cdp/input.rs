//! Interpreter events (`Ev`, client-area physical pixels, Windows virtual keys) → CDP `Input.*`
//! commands. Pure: no I/O, so all of it is unit-tested.
use crate::model::{Btn, Ev};
use serde_json::{Value, json};

/// A CDP command: method + params.
pub type Cmd = (&'static str, Value);

/// Maps client-area physical pixels (what blocks store) to the page.
/// The viewport sits at the bottom of the client area, under the browser's own bar (tabs,
/// address bar, Electron menu); CDP mouse events take CSS pixels, i.e. physical / devicePixelRatio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub off_x: i32,
    pub off_y: i32,
    pub dpr: f64,
}

impl Default for Geometry {
    fn default() -> Self {
        Geometry { off_x: 0, off_y: 0, dpr: 1.0 }
    }
}

impl Geometry {
    /// `client`: client-area size in physical pixels (`None` when the window is unknown: the
    /// coordinates are then taken as relative to the page); `inner`: `innerWidth/innerHeight`
    /// (CSS px); `dpr`: `devicePixelRatio` (includes the page zoom).
    pub fn new(client: Option<(i32, i32)>, inner: (f64, f64), dpr: f64) -> Self {
        let dpr = if dpr.is_finite() && dpr > 0.0 { dpr } else { 1.0 };
        let off_y = client.map_or(0, |(_, h)| (h as f64 - inner.1 * dpr).round().max(0.0) as i32);
        Geometry { off_x: 0, off_y, dpr }
    }

    /// Client px → CSS px of the viewport.
    pub fn to_css(&self, x: i32, y: i32) -> (f64, f64) {
        let r = |v: f64| (v * 100.0).round() / 100.0;
        (r((x - self.off_x) as f64 / self.dpr), r((y - self.off_y) as f64 / self.dpr))
    }

    /// Client px → pixel of `Page.captureScreenshot` (device pixels of the viewport).
    pub fn to_shot(&self, x: i32, y: i32) -> (i32, i32) {
        (x - self.off_x, y - self.off_y)
    }
}

/// DOM description of a Windows virtual key (US layout for punctuation).
#[derive(Debug, PartialEq)]
pub struct KeyDef {
    pub key: String,
    pub code: String,
    pub text: Option<String>,
    /// DOM keyCode: left/right variants collapse to the generic key (VK_LSHIFT → 16).
    pub key_code: u16,
    /// 0 standard, 1 left, 2 right, 3 numpad.
    pub location: u8,
}

pub fn key_def(vk: u16, shift: bool) -> KeyDef {
    let def = |key: &str, code: &str, text: Option<&str>, key_code: u16, location: u8| KeyDef {
        key: key.into(),
        code: code.into(),
        text: text.map(Into::into),
        key_code,
        location,
    };
    let named = |key: &str| def(key, key, None, vk, 0);
    let printable = |plain: char, shifted: char, code: &str| {
        let c = if shift { shifted } else { plain }.to_string();
        def(&c, code, Some(&c), vk, 0)
    };
    match vk {
        0x41..=0x5A => {
            let up = vk as u8 as char;
            printable(up.to_ascii_lowercase(), up, &format!("Key{up}"))
        }
        0x30..=0x39 => {
            let d = (vk - 0x30) as usize;
            printable((b'0' + d as u8) as char, ")!@#$%^&*(".as_bytes()[d] as char, &format!("Digit{d}"))
        }
        0x60..=0x69 => {
            let d = ((vk - 0x60) as u8 + b'0') as char;
            def(&d.to_string(), &format!("Numpad{d}"), Some(&d.to_string()), vk, 3)
        }
        0x6A => def("*", "NumpadMultiply", Some("*"), vk, 3),
        0x6B => def("+", "NumpadAdd", Some("+"), vk, 3),
        0x6D => def("-", "NumpadSubtract", Some("-"), vk, 3),
        0x6E => def(".", "NumpadDecimal", Some("."), vk, 3),
        0x6F => def("/", "NumpadDivide", Some("/"), vk, 3),
        0x70..=0x87 => named(&format!("F{}", vk - 0x6F)),
        0x20 => def(" ", "Space", Some(" "), vk, 0),
        0x0D => def("Enter", "Enter", Some("\r"), vk, 0),
        0x08 => named("Backspace"),
        0x09 => named("Tab"),
        0x1B => def("Escape", "Escape", None, vk, 0),
        0x13 => named("Pause"),
        0x14 => named("CapsLock"),
        0x21 => named("PageUp"),
        0x22 => named("PageDown"),
        0x23 => named("End"),
        0x24 => named("Home"),
        0x25 => named("ArrowLeft"),
        0x26 => named("ArrowUp"),
        0x27 => named("ArrowRight"),
        0x28 => named("ArrowDown"),
        0x2C => named("PrintScreen"),
        0x2D => named("Insert"),
        0x2E => named("Delete"),
        0x10 | 0xA0 => def("Shift", "ShiftLeft", None, 0x10, 1),
        0xA1 => def("Shift", "ShiftRight", None, 0x10, 2),
        0x11 | 0xA2 => def("Control", "ControlLeft", None, 0x11, 1),
        0xA3 => def("Control", "ControlRight", None, 0x11, 2),
        0x12 | 0xA4 => def("Alt", "AltLeft", None, 0x12, 1),
        0xA5 => def("Alt", "AltRight", None, 0x12, 2),
        0x5B => def("Meta", "MetaLeft", None, 0x5B, 1),
        0x5C => def("Meta", "MetaRight", None, 0x5C, 2),
        0xBA => printable(';', ':', "Semicolon"),
        0xBB => printable('=', '+', "Equal"),
        0xBC => printable(',', '<', "Comma"),
        0xBD => printable('-', '_', "Minus"),
        0xBE => printable('.', '>', "Period"),
        0xBF => printable('/', '?', "Slash"),
        0xC0 => printable('`', '~', "Backquote"),
        0xDB => printable('[', '{', "BracketLeft"),
        0xDC => printable('\\', '|', "Backslash"),
        0xDD => printable(']', '}', "BracketRight"),
        0xDE => printable('\'', '"', "Quote"),
        _ => def("Unidentified", "", None, vk, 0),
    }
}

/// CDP modifier bit of a modifier key: Alt=1, Ctrl=2, Meta=4, Shift=8.
pub fn modifier_bit(vk: u16) -> u32 {
    match vk {
        0x12 | 0xA4 | 0xA5 => 1,
        0x11 | 0xA2 | 0xA3 => 2,
        0x5B | 0x5C => 4,
        0x10 | 0xA0 | 0xA1 => 8,
        _ => 0,
    }
}

fn btn_name(b: Btn) -> &'static str {
    match b {
        Btn::Left => "left",
        Btn::Right => "right",
        Btn::Middle => "middle",
    }
}

fn btn_bit(b: Btn) -> u32 {
    match b {
        Btn::Left => 1,
        Btn::Right => 2,
        Btn::Middle => 4,
    }
}

/// Presses this close in time and space count as a double click (Chromium takes the click
/// count from the event; it doesn't infer it).
const DOUBLE_MS: u64 = 500;
const DOUBLE_PX: i32 = 4;

#[derive(Clone, Copy)]
struct Press {
    btn: Btn,
    x: i32,
    y: i32,
    t: u64,
    count: u32,
}

/// Mouse buttons and keys held by the script, so every event carries the right `buttons` and
/// `modifiers` (Key Ctrl Down, Key S Press, Key Ctrl Up → Ctrl+S).
#[derive(Default)]
pub struct InputState {
    keys: Vec<u16>,
    buttons: Vec<Btn>,
    pos: (i32, i32),
    last_press: Option<Press>,
}

impl InputState {
    pub fn modifiers(&self) -> u32 {
        self.keys.iter().fold(0, |m, &k| m | modifier_bit(k))
    }

    fn button_mask(&self) -> u32 {
        self.buttons.iter().fold(0, |m, &b| m | btn_bit(b))
    }

    fn mouse(&self, kind: &str, x: i32, y: i32, g: &Geometry, button: &str, count: u32) -> Cmd {
        let (cx, cy) = g.to_css(x, y);
        (
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": cx, "y": cy, "button": button, "buttons": self.button_mask(), "clickCount": count, "modifiers": self.modifiers() }),
        )
    }

    pub fn translate(&mut self, ev: &Ev, now_ms: u64, g: &Geometry) -> Vec<Cmd> {
        match *ev {
            Ev::Move(x, y) => {
                self.pos = (x, y);
                let held = self.buttons.first().map_or("none", |&b| btn_name(b));
                vec![self.mouse("mouseMoved", x, y, g, held, 0)]
            }
            Ev::Button(b, down, x, y) => {
                self.pos = (x, y);
                self.buttons.retain(|&o| o != b);
                let count = if down {
                    let count = match self.last_press {
                        Some(p) if p.btn == b && (p.x - x).abs() <= DOUBLE_PX && (p.y - y).abs() <= DOUBLE_PX && now_ms.saturating_sub(p.t) <= DOUBLE_MS => p.count + 1,
                        _ => 1,
                    };
                    self.last_press = Some(Press { btn: b, x, y, t: now_ms, count });
                    self.buttons.push(b);
                    count
                } else {
                    self.last_press.filter(|p| p.btn == b).map_or(1, |p| p.count)
                };
                vec![self.mouse(if down { "mousePressed" } else { "mouseReleased" }, x, y, g, btn_name(b), count)]
            }
            Ev::Wheel(delta, x, y) => {
                self.pos = (x, y);
                let (cx, cy) = g.to_css(x, y);
                // Windows: +120 per notch = up. CDP: CSS px, positive = down; Chromium scrolls ~100 px a notch.
                let dy = -(delta as f64) * 100.0 / 120.0;
                vec![("Input.dispatchMouseEvent", json!({ "type": "mouseWheel", "x": cx, "y": cy, "deltaX": 0, "deltaY": dy, "modifiers": self.modifiers() }))]
            }
            Ev::Key(vk, _, _, down) => {
                let repeat = down && self.keys.contains(&vk);
                if down && !repeat {
                    self.keys.push(vk); // a modifier's own keydown already carries its bit
                } else if !down {
                    self.keys.retain(|&k| k != vk);
                }
                let m = self.modifiers();
                let d = key_def(vk, m & 8 != 0);
                // With Ctrl/Alt/Meta held a key is a shortcut, not text (rawKeyDown runs the
                // page's keydown handlers and editing commands like Ctrl+A, but types nothing).
                let text = d.text.filter(|_| down && m & 0b111 == 0);
                let kind = match (down, &text) {
                    (false, _) => "keyUp",
                    (true, Some(_)) => "keyDown",
                    (true, None) => "rawKeyDown",
                };
                let mut p = json!({
                    "type": kind,
                    "modifiers": m,
                    "key": d.key,
                    "code": d.code,
                    "windowsVirtualKeyCode": d.key_code,
                    "nativeVirtualKeyCode": vk,
                    "location": d.location,
                    "isKeypad": d.location == 3,
                    "autoRepeat": repeat,
                });
                if let Some(t) = text {
                    p["text"] = json!(t);
                    p["unmodifiedText"] = json!(t);
                }
                vec![("Input.dispatchKeyEvent", p)]
            }
        }
    }

    /// Lifts every button and key the script left pressed (end of run, or stop).
    pub fn release_all(&mut self, now_ms: u64, g: &Geometry) -> Vec<Cmd> {
        let (x, y) = self.pos;
        let mut out = vec![];
        for b in self.buttons.clone().into_iter().rev() {
            out.extend(self.translate(&Ev::Button(b, false, x, y), now_ms, g));
        }
        for vk in self.keys.clone().into_iter().rev() {
            out.extend(self.translate(&Ev::Key(vk, 0, false, false), now_ms, g));
        }
        out
    }
}

pub fn insert_text(text: &str) -> Cmd {
    ("Input.insertText", json!({ "text": text }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Geometry = Geometry { off_x: 0, off_y: 0, dpr: 1.0 };

    fn p(c: &Cmd) -> &Value {
        &c.1
    }

    #[test]
    fn geometry_subtracts_bar_and_dpr() {
        // Chrome at 150 %: client 1200×900 physical, viewport 800×520 CSS → bar of 120 px.
        let g = Geometry::new(Some((1200, 900)), (800.0, 520.0), 1.5);
        assert_eq!(g, Geometry { off_x: 0, off_y: 120, dpr: 1.5 });
        assert_eq!(g.to_css(300, 270), (200.0, 100.0));
        assert_eq!(g.to_shot(300, 270), (300, 150));
        // Electron without menu: viewport fills the client area.
        assert_eq!(Geometry::new(Some((800, 600)), (800.0, 600.0), 1.0).off_y, 0);
        // Window unknown, or bogus numbers: no offset, ratio 1.
        assert_eq!(Geometry::new(None, (800.0, 600.0), 2.0), Geometry { off_x: 0, off_y: 0, dpr: 2.0 });
        assert_eq!(Geometry::new(Some((100, 100)), (800.0, 600.0), 0.0), Geometry::default());
        assert_eq!(Geometry::new(Some((3, 3)), (1.0, 1.0), 3.0).to_css(1, 2), (0.33, 0.67));
    }

    #[test]
    fn click_becomes_pressed_released_in_css_px() {
        let g = Geometry::new(Some((1000, 700)), (500.0, 300.0), 2.0); // bar 100 px
        let mut s = InputState::default();
        let mv = s.translate(&Ev::Move(200, 300), 0, &g);
        assert_eq!(mv[0].0, "Input.dispatchMouseEvent");
        assert_eq!(p(&mv[0]), &json!({"type":"mouseMoved","x":100.0,"y":100.0,"button":"none","buttons":0,"clickCount":0,"modifiers":0}));
        let down = s.translate(&Ev::Button(Btn::Left, true, 200, 300), 0, &g);
        assert_eq!(p(&down[0]), &json!({"type":"mousePressed","x":100.0,"y":100.0,"button":"left","buttons":1,"clickCount":1,"modifiers":0}));
        let drag = s.translate(&Ev::Move(210, 300), 10, &g);
        assert_eq!((p(&drag[0])["button"].clone(), p(&drag[0])["buttons"].clone()), (json!("left"), json!(1)));
        let up = s.translate(&Ev::Button(Btn::Left, false, 210, 300), 30, &g);
        assert_eq!(p(&up[0]), &json!({"type":"mouseReleased","x":105.0,"y":100.0,"button":"left","buttons":0,"clickCount":1,"modifiers":0}));
    }

    #[test]
    fn double_click_counts() {
        let mut s = InputState::default();
        let count = |s: &mut InputState, b, down, x, t| p(&s.translate(&Ev::Button(b, down, x, 10), t, &G)[0])["clickCount"].as_u64().unwrap();
        assert_eq!(count(&mut s, Btn::Left, true, 10, 0), 1);
        assert_eq!(count(&mut s, Btn::Left, false, 10, 30), 1);
        assert_eq!(count(&mut s, Btn::Left, true, 11, 60), 2);
        assert_eq!(count(&mut s, Btn::Left, false, 11, 90), 2);
        assert_eq!(count(&mut s, Btn::Left, true, 11, 2000), 1, "too late");
        assert_eq!(count(&mut s, Btn::Left, true, 50, 2030), 1, "too far");
        assert_eq!(count(&mut s, Btn::Right, true, 50, 2060), 1, "other button");
    }

    #[test]
    fn wheel_flips_sign() {
        let mut s = InputState::default();
        let w = s.translate(&Ev::Wheel(-120, 5, 6), 0, &G);
        assert_eq!(p(&w[0]), &json!({"type":"mouseWheel","x":5.0,"y":6.0,"deltaX":0,"deltaY":100.0,"modifiers":0}));
    }

    #[test]
    fn ctrl_s_carries_modifier_and_no_text() {
        let mut s = InputState::default();
        let ks: Vec<Cmd> = [Ev::Key(0x11, 0, false, true), Ev::Key(0x53, 0, false, true), Ev::Key(0x53, 0, false, false), Ev::Key(0x11, 0, false, false)]
            .iter()
            .flat_map(|e| s.translate(e, 0, &G))
            .collect();
        let v: Vec<(&str, &str, u64)> = ks.iter().map(|c| (p(c)["type"].as_str().unwrap(), p(c)["key"].as_str().unwrap(), p(c)["modifiers"].as_u64().unwrap())).collect();
        assert_eq!(v, [("rawKeyDown", "Control", 2), ("rawKeyDown", "s", 2), ("keyUp", "s", 2), ("keyUp", "Control", 0)]);
        assert_eq!(p(&ks[1])["code"], "KeyS");
        assert_eq!(p(&ks[1])["windowsVirtualKeyCode"], 0x53);
        assert!(p(&ks[1]).get("text").is_none());
        assert_eq!(p(&ks[0])["location"], 1);
    }

    #[test]
    fn typing_keys_and_shift() {
        let mut s = InputState::default();
        let a = s.translate(&Ev::Key(0x41, 0, false, true), 0, &G);
        assert_eq!((p(&a[0])["type"].as_str(), p(&a[0])["text"].as_str()), (Some("keyDown"), Some("a")));
        let again = s.translate(&Ev::Key(0x41, 0, false, true), 0, &G);
        assert_eq!(p(&again[0])["autoRepeat"], true);
        s.translate(&Ev::Key(0x41, 0, false, false), 0, &G);
        s.translate(&Ev::Key(0xA0, 0, false, true), 0, &G); // LShift
        let one = s.translate(&Ev::Key(0x31, 0, false, true), 0, &G);
        assert_eq!((p(&one[0])["key"].as_str(), p(&one[0])["text"].as_str(), p(&one[0])["modifiers"].as_u64()), (Some("!"), Some("!"), Some(8)));
        s.translate(&Ev::Key(0x31, 0, false, false), 0, &G);
        let enter = s.translate(&Ev::Key(0x0D, 0, false, true), 0, &G);
        assert_eq!(p(&enter[0])["text"], "\r");
        // Release everything still held: Enter, then Shift (reverse order), Shift last clears bit 8.
        let rel = s.release_all(0, &G);
        let keys: Vec<(&str, u64)> = rel.iter().map(|c| (p(c)["key"].as_str().unwrap(), p(c)["modifiers"].as_u64().unwrap())).collect();
        assert_eq!(keys, [("Enter", 8), ("Shift", 0)]);
        assert!(s.release_all(0, &G).is_empty());
    }

    #[test]
    fn key_table() {
        assert_eq!(key_def(0x25, false), KeyDef { key: "ArrowLeft".into(), code: "ArrowLeft".into(), text: None, key_code: 0x25, location: 0 });
        assert_eq!(key_def(0x70, false).key, "F1");
        assert_eq!(key_def(0x7B, false).code, "F12");
        assert_eq!(key_def(0x61, false), KeyDef { key: "1".into(), code: "Numpad1".into(), text: Some("1".into()), key_code: 0x61, location: 3 });
        assert_eq!(key_def(0xA3, false).key_code, 0x11);
        assert_eq!(key_def(0xBF, true).text.as_deref(), Some("?"));
        assert_eq!(key_def(0x09, false).text, None);
        assert_eq!(key_def(0xFF, false).key, "Unidentified");
        assert_eq!([0x12, 0xA2, 0x5B, 0xA1, 0x41].map(modifier_bit), [1, 2, 4, 8, 0]);
    }

    #[test]
    fn modifiers_reach_mouse_and_release_buttons() {
        let mut s = InputState::default();
        s.translate(&Ev::Key(0x11, 0, false, true), 0, &G);
        let c = s.translate(&Ev::Button(Btn::Middle, true, 7, 8), 0, &G);
        assert_eq!((p(&c[0])["modifiers"].as_u64(), p(&c[0])["buttons"].as_u64()), (Some(2), Some(4)));
        let rel = s.release_all(100, &G);
        assert_eq!(p(&rel[0])["type"], "mouseReleased");
        assert_eq!(p(&rel[1])["type"], "keyUp");
        assert_eq!(insert_text("olá"), ("Input.insertText", json!({"text":"olá"})));
    }
}
