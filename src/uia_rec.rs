//! The UIA element under each recorded click. The hook thread only queues the click; a
//! collector thread resolves it through a UIA worker (with timeout), and "Explodir em
//! passos" turns clicks on actionable elements into UIA blocks.
use crate::model::{Block, Btn, ClickKind, Ev, Event};
use crate::uia::{Captured, UiaAction, UiaElem, capture, default_timeout, session};
use crate::win::Hwnd;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{SendError, Sender, channel};
use std::time::Duration;

/// Resolving one click's element while recording.
const CLICK_TIMEOUT: Duration = Duration::from_secs(2);

/// The element under one recorded click (`Recording.elements`).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecElem {
    /// Index of the button-down event in `events`.
    pub event: u32,
    pub elem: UiaElem,
    /// Set when the element can be driven without the mouse (Invoke, Toggle or Select):
    /// "Explodir em passos" then turns the click into a UIA block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<UiaAction>,
}

fn downs(events: &[Event]) -> Vec<usize> {
    events.iter().enumerate().filter(|(_, e)| matches!(e.ev, Ev::Button(_, true, _, _))).map(|(i, _)| i).collect()
}

/// `(n-th button-down, element)` pairs → `Recording.elements` (indices into `events`).
/// Cleaning a recording only drops key events, so the n-th button-down is stable.
pub fn rec_elements(events: &[Event], got: Vec<(u32, Captured)>) -> Vec<RecElem> {
    let downs = downs(events);
    got.into_iter()
        .filter_map(|(ord, c)| downs.get(ord as usize).map(|&i| RecElem { event: i as u32, elem: c.elem, action: c.action }))
        .collect()
}

/// After `runner::explode`: each simple left Click whose element can be driven without
/// input becomes a UIA block that falls back to clicking the same point. The blocks of
/// `explode` consume button-downs in order (Click/Down one, Double two, Up none).
pub fn upgrade_clicks(mut blocks: Vec<Block>, events: &[Event], elems: &[RecElem]) -> Vec<Block> {
    if elems.is_empty() {
        return blocks;
    }
    let downs = downs(events);
    let mut k = 0;
    for b in &mut blocks {
        let Block::Click { kind, button, .. } = b else { continue };
        let first = k;
        k += match kind {
            ClickKind::Double => 2,
            ClickKind::Up => 0,
            ClickKind::Click | ClickKind::Down => 1,
        };
        if *kind != ClickKind::Click || *button != Btn::Left {
            continue;
        }
        let Some(&ev) = downs.get(first) else { continue };
        if let Some(r) = elems.iter().find(|r| r.event as usize == ev)
            && let Some(action) = &r.action
        {
            *b = Block::Uia { elem: r.elem.clone(), action: action.clone(), timeout_ms: default_timeout(), fallback_click: true };
        }
    }
    blocks
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    /// n-th button-down of the recording.
    pub ord: u32,
    /// Screen point (for ElementFromPoint) and the same point in client coordinates.
    pub sx: i32,
    pub sy: i32,
    pub cx: i32,
    pub cy: i32,
}

type Done = Box<dyn FnOnce(Vec<(u32, Captured)>) + Send>;

enum Msg {
    Click(Click),
    Finish(Done),
}

/// Resolves the element of each recorded click on its own thread. The hook only queues
/// (`click` is a channel send), so recording never waits for UI Automation.
pub struct ClickCollector {
    tx: Sender<Msg>,
}

impl ClickCollector {
    pub fn start(mut resolve: impl FnMut(&Click) -> Option<Captured> + Send + 'static) -> Self {
        let (tx, rx) = channel::<Msg>();
        std::thread::spawn(move || {
            let mut got = vec![];
            for m in rx {
                match m {
                    Msg::Click(c) => {
                        // A panicking resolver must not lose the recording.
                        if let Ok(Some(cap)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| resolve(&c))) {
                            got.push((c.ord, cap));
                        }
                    }
                    Msg::Finish(done) => return done(std::mem::take(&mut got)),
                }
            }
        });
        ClickCollector { tx }
    }

    /// Hook thread: queue and return immediately.
    pub fn click(&self, c: Click) {
        let _ = self.tx.send(Msg::Click(c));
    }

    /// `done` runs on the collector thread once every queued click is resolved.
    pub fn finish(self, done: impl FnOnce(Vec<(u32, Captured)>) + Send + 'static) {
        if let Err(SendError(Msg::Finish(done))) = self.tx.send(Msg::Finish(Box::new(done))) {
            done(vec![]);
        }
    }
}

/// One collector per recording, resolving clicks on window `root` through a UIA worker.
pub fn click_collector(root: Hwnd) -> ClickCollector {
    let mut w = session();
    ClickCollector::start(move |c| {
        let c = *c;
        w.call(CLICK_TIMEOUT, move |ctx| capture(ctx, root, c.sx, c.sy, c.cx, c.cy)).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn recorded_clicks_become_uia_blocks() {
        let e = |t, ev| Event { t, ev };
        let events = vec![
            e(0, Ev::Button(Btn::Left, true, 10, 10)), // 0: click on "Salvar" (Invoke)
            e(50, Ev::Button(Btn::Left, false, 10, 10)),
            e(1000, Ev::Button(Btn::Left, true, 20, 20)), // 2: double click on a list item
            e(1050, Ev::Button(Btn::Left, false, 20, 20)),
            e(1150, Ev::Button(Btn::Left, true, 20, 20)), // 4
            e(1200, Ev::Button(Btn::Left, false, 20, 20)),
            e(2000, Ev::Button(Btn::Left, true, 30, 30)), // 6: drag
            e(2100, Ev::Move(60, 60)),
            e(2200, Ev::Button(Btn::Left, false, 90, 90)),
            e(3000, Ev::Button(Btn::Left, true, 40, 40)), // 9: click on a checkbox (Toggle)
            e(3050, Ev::Button(Btn::Left, false, 40, 40)),
            e(4000, Ev::Button(Btn::Right, true, 50, 50)), // 11: right click (stays a click)
            e(4050, Ev::Button(Btn::Right, false, 50, 50)),
            e(5000, Ev::Button(Btn::Left, true, 60, 60)), // 13: click on text (no action)
            e(5050, Ev::Button(Btn::Left, false, 60, 60)),
        ];
        let cap = |ct: &str, x, y, action| Captured { elem: UiaElem { control_type: ct.into(), name: ct.into(), x, y, ..Default::default() }, action };
        let got = vec![
            (0, cap("Button", 10, 10, Some(UiaAction::Invoke))),
            (1, cap("ListItem", 20, 20, Some(UiaAction::Select))),
            (2, cap("ListItem", 20, 20, Some(UiaAction::Select))),
            (3, cap("Pane", 30, 30, Some(UiaAction::Invoke))),
            (4, cap("CheckBox", 40, 40, Some(UiaAction::Toggle))),
            (5, cap("Button", 50, 50, Some(UiaAction::Invoke))),
            (6, cap("Text", 60, 60, None)),
            (99, cap("Button", 0, 0, Some(UiaAction::Invoke))), // no such click: dropped
        ];
        let elems = rec_elements(&events, got);
        assert_eq!(elems.iter().map(|r| r.event).collect::<Vec<_>>(), [0, 2, 4, 6, 9, 11, 13]);
        let out = upgrade_clicks(crate::runner::explode(&events), &events, &elems);
        let kinds: Vec<String> = out
            .iter()
            .filter_map(|b| match b {
                Block::Click { kind, button, .. } => Some(format!("{kind:?}{button:?}")),
                Block::Uia { elem, action, fallback_click: true, .. } => Some(format!("Uia {} {action:?} ({}, {})", elem.control_type, elem.x, elem.y)),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, ["Uia Button Invoke (10, 10)", "DoubleLeft", "DownLeft", "UpLeft", "Uia CheckBox Toggle (40, 40)", "ClickRight", "ClickLeft"]);
        // The Waits between steps are kept as they were.
        let waits = |v: &[Block]| v.iter().filter(|b| matches!(b, Block::Wait { .. })).count();
        assert_eq!(waits(&out), waits(&crate::runner::explode(&events)));
        // No elements (recordings made before ticket 17): explode output is untouched.
        assert_eq!(upgrade_clicks(crate::runner::explode(&events), &events, &[]), crate::runner::explode(&events));
    }

    #[test]
    fn rec_elem_json() {
        let r = RecElem { event: 3, elem: UiaElem { name: "OK".into(), control_type: "Button".into(), ..Default::default() }, action: Some(UiaAction::Invoke) };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<RecElem>(&s).unwrap(), r);
        let no_action: RecElem = serde_json::from_str(r#"{"event":0,"elem":{"name":"x"}}"#).unwrap();
        assert_eq!(no_action.action, None);
        assert!(!serde_json::to_string(&no_action).unwrap().contains("action"));
        assert!(serde_json::from_str::<RecElem>(r#"{"event":0,"elem":{},"acao":"Invoke"}"#).is_err());
    }

    /// The hook only queues. 200 clicks with a resolver that takes 20 ms each (4 s in
    /// total) are queued in well under the time of a single resolution.
    #[test]
    fn collector_never_blocks_the_hook() {
        let col = ClickCollector::start(|c: &Click| {
            std::thread::sleep(Duration::from_millis(20));
            Some(Captured { elem: UiaElem { name: format!("b{}", c.ord), x: c.cx, y: c.cy, ..Default::default() }, action: None })
        });
        let t0 = Instant::now();
        let mut worst = Duration::ZERO;
        for ord in 0..200 {
            let t = Instant::now();
            col.click(Click { ord, sx: 0, sy: 0, cx: ord as i32, cy: 0 });
            worst = worst.max(t.elapsed());
        }
        let total = t0.elapsed();
        eprintln!("hook: 200 cliques enfileirados em {total:?}, pior clique {worst:?}");
        assert!(total < Duration::from_millis(20), "200 cliques enfileirados em {total:?}");
        let (tx, rx) = channel();
        col.finish(move |got| tx.send(got).unwrap());
        let got = rx.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(got.len(), 200);
        assert!(got.iter().enumerate().all(|(i, (ord, c))| *ord == i as u32 && c.elem.x == i as i32));
        // A panicking resolver still finishes the recording.
        let col = ClickCollector::start(|_: &Click| panic!("boom"));
        col.click(Click { ord: 0, sx: 0, sy: 0, cx: 0, cy: 0 });
        let (tx, rx) = channel();
        col.finish(move |got| tx.send(got.len()).unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 0);
    }
}
